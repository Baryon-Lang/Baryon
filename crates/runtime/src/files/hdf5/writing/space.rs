//! Where each structure of a new file goes: an emulation of how the HDF5 library 2.0 hands out
//! file space with its default settings, so a file written here puts every structure at the
//! address h5py's would. Those settings are the `sec2` driver, the `FSM_AGGR` strategy, a
//! free-space threshold of 1 byte, no alignment, and free space not kept in the file.
//!
//! The library takes space in three ways, tried in order:
//!
//! - a *free-space manager*, one for metadata and one for raw data, holds the sections freed
//!   while the file is open, each merged with its neighbours, and gives out the smallest one
//!   that fits (`H5MF_alloc`). It starts with the first section freed that cannot go back to
//!   the end of the file or to an aggregator;
//! - an *aggregator*, one for metadata and one for small raw data, holds a 2048-byte block
//!   taken from the end of the file and hands it out front to back (`H5MF__aggr_alloc`). A
//!   request of a block or more takes space at the end of the file directly;
//! - the end of the file, which grows.
//!
//! Space freed at the end of the file shrinks it, and space next to an aggregator joins it. A
//! block that grows in place (a local heap's data, an object header's chunk, a global heap
//! collection) grows at the end of the file, into its aggregator, or into a free section just
//! after it (`H5MF_try_extend`). Closing the file frees both aggregators and shrinks the file
//! past any free space at its end, twice (`H5MF_close`).

/// The undefined address.
pub(super) const UNDEFINED: u64 = u64::MAX;

/// The size of the block an aggregator takes from the end of the file.
const BLOCK: u64 = 2048;

/// What a piece of space holds, which decides the free-space manager and the aggregator it
/// comes from and goes back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Use {
    /// Metadata: the superblock, object headers, B-tree nodes, local heaps, and an
    /// aggregator's leftover metadata space.
    Metadata,
    /// Raw data: datasets' values, global heap collections, and an aggregator's leftover raw
    /// space.
    Raw,
}

/// An aggregator: a block taken from the end of the file and handed out front to back.
#[derive(Clone, Copy, Debug)]
struct Aggregator {
    /// The start of the space not handed out yet, [`UNDEFINED`] once freed for good.
    address: u64,
    /// The bytes not handed out yet.
    size: u64,
    /// The bytes taken from the end of the file since it last started a block.
    total: u64,
}

impl Aggregator {
    /// An aggregator that holds nothing yet.
    const EMPTY: Self = Self {
        address: 0,
        size: 0,
        total: 0,
    };
}

/// The file's space: its end, its two aggregators and its two free-space managers.
#[derive(Debug)]
pub(super) struct Space {
    /// The end of the file: the first address no structure has taken.
    pub(super) end: u64,
    /// The aggregator for metadata.
    metadata: Aggregator,
    /// The aggregator for small raw data.
    raw: Aggregator,
    /// The free sections, `(address, size)`, of each use: metadata first, then raw. `None`
    /// until the manager starts.
    free: [Option<Vec<(u64, u64)>>; 2],
}

impl Space {
    /// The space of a new, empty file.
    pub(super) fn new() -> Self {
        Self {
            end: 0,
            metadata: Aggregator::EMPTY,
            raw: Aggregator::EMPTY,
            free: [None, None],
        }
    }

    /// The aggregator for `kind` and the other one.
    fn aggregators(&mut self, kind: Use) -> (&mut Aggregator, &mut Aggregator) {
        match kind {
            Use::Metadata => (&mut self.metadata, &mut self.raw),
            Use::Raw => (&mut self.raw, &mut self.metadata),
        }
    }

    /// The free sections of `kind`, `None` until its manager starts.
    fn sections(&mut self, kind: Use) -> &mut Option<Vec<(u64, u64)>> {
        &mut self.free[usize::from(kind == Use::Raw)]
    }

    /// Takes `size` bytes for `kind` and returns their address (`H5MF_alloc`).
    pub(super) fn allocate(&mut self, kind: Use, size: u64) -> u64 {
        if let Some(address) = self.best_fit(kind, size) {
            return address;
        }
        self.aggregate(kind, size)
    }

    /// Takes the smallest free section of `kind` that holds `size` bytes, lowest address
    /// first among equals, and frees what it does not use.
    fn best_fit(&mut self, kind: Use, size: u64) -> Option<u64> {
        let sections = self.sections(kind).as_mut()?;
        let (index, &(address, length)) = sections
            .iter()
            .enumerate()
            .filter(|(_, section)| section.1 >= size)
            .min_by_key(|(_, section)| (section.1, section.0))?;
        sections.remove(index);
        if length > size {
            self.add_section(kind, address + size, length - size);
        }
        Some(address)
    }

    /// Takes `size` bytes from the aggregator of `kind` (`H5MF__aggr_alloc`).
    fn aggregate(&mut self, kind: Use, size: u64) -> u64 {
        let end = self.end;
        let (aggregator, other) = self.aggregators(kind);
        let (aggregator, other) = (*aggregator, *other);
        if size <= aggregator.size {
            let (aggregator, _) = self.aggregators(kind);
            aggregator.size -= size;
            aggregator.address += size;
            return aggregator.address - size;
        }
        // The other aggregator gives its space back when it ends the file and holds at least
        // a block more than it has handed out.
        let other_gives_back = other.size > 0
            && other.address + other.size == end
            && other.total > other.size
            && other.total - other.size >= BLOCK;
        let started = aggregator.address > 0 && aggregator.address != UNDEFINED;
        if size >= BLOCK {
            if started && self.extend_end(aggregator.address + aggregator.size, size) {
                let (aggregator, _) = self.aggregators(kind);
                aggregator.address += size;
                aggregator.total += size;
                return aggregator.address - size;
            }
            if other_gives_back {
                self.free_aggregator(kind, true);
            }
            let address = self.end;
            self.end += size;
            return address;
        }
        if started && self.extend_end(aggregator.address + aggregator.size, BLOCK) {
            let (aggregator, _) = self.aggregators(kind);
            aggregator.size += BLOCK;
            aggregator.total += BLOCK;
        } else {
            if other_gives_back {
                self.free_aggregator(kind, true);
            }
            let block = self.end;
            self.end += BLOCK;
            if aggregator.size > 0 {
                self.free(kind, aggregator.address, aggregator.size);
            }
            let (aggregator, _) = self.aggregators(kind);
            *aggregator = Aggregator {
                address: block,
                size: BLOCK,
                total: BLOCK,
            };
        }
        let (aggregator, _) = self.aggregators(kind);
        aggregator.size -= size;
        aggregator.address += size;
        aggregator.address - size
    }

    /// Grows the end of the file by `extra` bytes when `block_end` is the end.
    fn extend_end(&mut self, block_end: u64, extra: u64) -> bool {
        if block_end == self.end {
            self.end += extra;
            return true;
        }
        false
    }

    /// Frees an aggregator's space for good: the aggregator of `kind`, or the other one when
    /// `other` (`H5MF__aggr_free`).
    fn free_aggregator(&mut self, kind: Use, other: bool) {
        let (mine, theirs) = self.aggregators(kind);
        let aggregator = if other { theirs } else { mine };
        let (address, size) = (aggregator.address, aggregator.size);
        *aggregator = Aggregator {
            address: UNDEFINED,
            size: 0,
            total: 0,
        };
        if address + size == self.end {
            self.end = address;
        }
    }

    /// Gives back `size` bytes of `kind` at `address` (`H5MF_xfree`).
    pub(super) fn free(&mut self, kind: Use, address: u64, size: u64) {
        if size == 0 || address == UNDEFINED {
            return;
        }
        if self.sections(kind).is_none() {
            if self.shrink(kind, (address, size), false).0 {
                return;
            }
            *self.sections(kind) = Some(Vec::new());
        }
        self.add_section(kind, address, size);
    }

    /// Adds a free section, merged with its neighbours, then shrinks the file or joins an
    /// aggregator with it when it touches one (`H5FS_sect_add`).
    fn add_section(&mut self, kind: Use, address: u64, size: u64) {
        let mut section = (address, size);
        loop {
            let sections = self.sections(kind).get_or_insert_with(Vec::new);
            while let Some(index) = sections.iter().position(|other| {
                other.0 + other.1 == section.0 || section.0 + section.1 == other.0
            }) {
                let other = sections.remove(index);
                section = (section.0.min(other.0), section.1 + other.1);
            }
            match self.shrink(kind, section, true) {
                (true, None) => return,
                (true, Some(rest)) => section = rest,
                (false, _) => break,
            }
        }
        self.sections(kind)
            .get_or_insert_with(Vec::new)
            .push(section);
    }

    /// Gives `section` back to the end of the file, or to the aggregator of `kind` when it
    /// touches it. Returns whether it went, and the bigger section left when the aggregator
    /// joined it instead, which `absorb` allows (`H5MF__sect_simple_can_shrink` and
    /// `H5MF__sect_simple_shrink`).
    fn shrink(
        &mut self,
        kind: Use,
        section: (u64, u64),
        absorb: bool,
    ) -> (bool, Option<(u64, u64)>) {
        let (address, size) = section;
        if address + size == self.end {
            self.end = address;
            return (true, None);
        }
        let (aggregator, _) = self.aggregators(kind);
        if aggregator.address == UNDEFINED
            || !(address + size == aggregator.address
                || aggregator.address + aggregator.size == address)
        {
            return (false, None);
        }
        if aggregator.size + size >= BLOCK && absorb {
            let joined = if address + size == aggregator.address {
                (address, size + aggregator.size)
            } else {
                (address - aggregator.size, size + aggregator.size)
            };
            *aggregator = Aggregator::EMPTY;
            return (true, Some(joined));
        }
        if address + size == aggregator.address {
            aggregator.address -= size;
            aggregator.size += size;
            aggregator.total -= aggregator.total.min(size);
        } else {
            aggregator.size += size;
        }
        (true, None)
    }

    /// Grows the block of `kind` at `address`, `size` bytes long, by `extra` bytes in place.
    /// Returns whether it grew (`H5MF_try_extend`).
    pub(super) fn try_extend(&mut self, kind: Use, address: u64, size: u64, extra: u64) -> bool {
        let end = address + size;
        if self.extend_end(end, extra) {
            return true;
        }
        let file_end = self.end;
        let (aggregator, _) = self.aggregators(kind);
        if end == aggregator.address {
            if file_end == aggregator.address + aggregator.size {
                // At most a tenth of the aggregator comes from it; more grows the file.
                if extra.saturating_mul(10) <= aggregator.size {
                    aggregator.size -= extra;
                    aggregator.address += extra;
                    return true;
                }
                let more = extra.max(BLOCK);
                let aggregator_end = aggregator.address + aggregator.size;
                if self.extend_end(aggregator_end, more) {
                    let (aggregator, _) = self.aggregators(kind);
                    aggregator.address += extra;
                    aggregator.total += more;
                    aggregator.size = aggregator.size + more - extra;
                    return true;
                }
            } else if aggregator.size >= extra {
                aggregator.size -= extra;
                aggregator.address += extra;
                return true;
            }
        }
        let Some(sections) = self.sections(kind).as_mut() else {
            return false;
        };
        let Some(index) = sections
            .iter()
            .position(|section| section.0 == end && section.1 >= extra)
        else {
            return false;
        };
        let (_, length) = sections.remove(index);
        if length > extra {
            self.add_section(kind, end + extra, length - extra);
        }
        true
    }

    /// Closes the file's space: frees both aggregators, the lower one first, shrinks the file
    /// past free space at its end, forgets the free sections, and does it again
    /// (`H5MF_close`).
    pub(super) fn close(&mut self) {
        let raw_first = self.metadata.address != UNDEFINED
            && self.raw.address != UNDEFINED
            && self.metadata.address < self.raw.address;
        let order = if raw_first {
            [Use::Raw, Use::Metadata]
        } else {
            [Use::Metadata, Use::Raw]
        };
        self.close_round(order);
        self.free = [None, None];
        self.close_round(order);
    }

    /// One round of [`Space::close`].
    fn close_round(&mut self, order: [Use; 2]) {
        for kind in order {
            let (aggregator, _) = self.aggregators(kind);
            let (address, size) = (aggregator.address, aggregator.size);
            *aggregator = Aggregator::EMPTY;
            if size > 0 {
                self.free(kind, address, size);
            }
        }
        let mut shrank = true;
        while shrank {
            shrank = false;
            for kind in [Use::Metadata, Use::Raw] {
                let end = self.end;
                if let Some(sections) = self.sections(kind).as_mut()
                    && let Some(index) = sections.iter().position(|s| s.0 + s.1 == end)
                {
                    let (address, _) = sections.remove(index);
                    self.end = address;
                    shrank = true;
                }
            }
            for kind in [Use::Metadata, Use::Raw] {
                let end = self.end;
                let (aggregator, _) = self.aggregators(kind);
                if aggregator.size > 0
                    && aggregator.address != UNDEFINED
                    && aggregator.address + aggregator.size == end
                {
                    self.free_aggregator(kind, false);
                    shrank = true;
                }
            }
        }
    }
}
