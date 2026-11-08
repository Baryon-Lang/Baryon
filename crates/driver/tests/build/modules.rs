//! Programs of several modules built and run, naming another module's items through it.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// A `Final` constant and an enum member of another module, read as `u.G`, `units.G` and
/// `units.Phase.GAS`. Before this, both were reported as attributes lowering does not handle;
/// only names imported with `from ... import` were read.
#[test]
fn constants_and_members_named_through_their_module() {
    let units = "\
G: Final = 9.81


class Phase(Enum):
    SOLID = 1
    GAS = 2
";
    let program = "\
import physics.units as u
from physics import units


def main():
    print(u.G * 2.0, units.G)
    p = units.Phase.GAS
    if p == u.Phase.GAS:
        print(\"gas\")
";
    let directory = package(
        "modules",
        &[("physics/units.byn", units), ("main.byn", program)],
    )
    .unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&run), "");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "19.62 9.81\ngas\n");
}
