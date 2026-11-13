! Lennard-Jones molecular dynamics on several threads, with a half neighbour list: the same liquid,
! lattice, velocities and steps as `lennard_jones.f90`, but each pair is computed once and its force
! added into both atoms, as Newton's third law allows.
!
! Usage: lennard_jones_half <cells per side> <steps> <threads>
!   The crystal starts as a face-centred cubic lattice of 4 * cells^3 atoms. Use 5 or more cells
!   so the force cell grid is at least 3 wide. `threads` OpenMP threads share the force loop.
! Stresses: adding into a shared array from parallel iterations, the pattern Baryon writes as
!   `prange(n, accumulate=forces)` (decision 0013); half the pair arithmetic of `lennard_jones`.
! Checksum: total energy per atom after the last step, in reduced units.
!
! Why not `reduction(+: fx)`: OpenMP leaves the order in which threads' copies are combined to the
! runtime, so two runs could differ in the last bits, and molecular dynamics grows that difference.
! The harness requires repeated runs to agree to the bit. So each thread adds into its own copy of
! the force arrays, which is what an OpenMP array reduction does, and the copies are summed in thread
! order afterwards. For a given thread count the result is the same on every run; another thread
! count changes only the rounding. `lennard_jones_half.cpp` does the same with `std::thread`.

program lennard_jones_half
    use support
    use, intrinsic :: iso_fortran_env, only: error_unit
    use omp_lib, only: omp_get_thread_num
    implicit none

    real(real64), parameter :: density = 0.8442_real64
    real(real64), parameter :: temperature = 0.72_real64
    real(real64), parameter :: cutoff = 2.5_real64
    real(real64), parameter :: time_step = 0.005_real64
    character(len=*), parameter :: usage = '<cells per side> <steps> <threads>'

    integer(int64) :: lattice_cells, steps, step, atoms, cells, threads
    real(real64) :: box, potential, start, elapsed
    real(real64), allocatable :: x(:), y(:), z(:), vx(:), vy(:), vz(:), fx(:), fy(:), fz(:)
    ! Linked list of atoms per force cell; 0 ends a list.
    integer(int64), allocatable :: head(:), next(:)
    ! Each thread's own copy of the forces, one column per thread, and its share of the energy.
    real(real64), allocatable :: thread_fx(:, :), thread_fy(:, :), thread_fz(:, :), thread_energy(:)

    lattice_cells = argument(1, usage)
    steps = argument(2, usage)
    threads = argument(3, usage)

    call initialize()
    if (cells < 3) then
        write (error_unit, '(a)') 'lennard_jones_half: use at least 5 cells per side'
        error stop 2
    end if

    start = seconds()
    potential = compute_forces()
    do step = 1, steps
        vx = vx + 0.5_real64*time_step*fx
        vy = vy + 0.5_real64*time_step*fy
        vz = vz + 0.5_real64*time_step*fz
        x = wrap(x + time_step*vx)
        y = wrap(y + time_step*vy)
        z = wrap(z + time_step*vz)
        potential = compute_forces()
        vx = vx + 0.5_real64*time_step*fx
        vy = vy + 0.5_real64*time_step*fy
        vz = vz + 0.5_real64*time_step*fz
    end do
    elapsed = seconds() - start

    call report((kinetic_energy() + potential)/real(atoms, real64), elapsed)

contains

    ! Wraps a coordinate into [0, box).
    elemental function wrap(coordinate) result(wrapped)
        real(real64), intent(in) :: coordinate
        real(real64) :: wrapped

        wrapped = coordinate - box*real(floor(coordinate/box, int64), real64)
    end function wrap

    ! The zero-based cell index along one axis of a wrapped coordinate.
    function cell_of(coordinate) result(cell)
        real(real64), intent(in) :: coordinate
        integer(int64) :: cell

        cell = min(int(coordinate/box*real(cells, real64), int64), cells - 1)
    end function cell_of

    ! Sets every force and returns the potential energy. Each pair is visited once, from the atom
    ! with the lower index, and its force is added into both atoms with opposite signs. Thread `t`
    ! takes a contiguous range of atoms and adds into column `t` of the thread arrays; the columns
    ! are then summed in thread order, so the result does not depend on how the threads ran.
    function compute_forces() result(energy)
        real(real64) :: energy
        integer(int64) :: i, j, t, cell, neighbor, cx, cy, cz, dx, dy, dz, first, last
        real(real64) :: rx, ry, rz, r2, inverse_r2, inverse_r6, magnitude, partial

        head = 0
        do i = 1, atoms
            cell = (cell_of(z(i))*cells + cell_of(y(i)))*cells + cell_of(x(i)) + 1
            next(i) = head(cell)
            head(cell) = i
        end do

        !$omp parallel num_threads(threads) default(shared) &
        !$omp private(i, j, t, neighbor, cx, cy, cz, dx, dy, dz, first, last) &
        !$omp private(rx, ry, rz, r2, inverse_r2, inverse_r6, magnitude, partial)
        t = int(omp_get_thread_num(), int64) + 1
        first = (t - 1)*atoms/threads + 1
        last = t*atoms/threads
        thread_fx(:, t) = 0.0_real64
        thread_fy(:, t) = 0.0_real64
        thread_fz(:, t) = 0.0_real64
        partial = 0.0_real64
        do i = first, last
            cx = cell_of(x(i))
            cy = cell_of(y(i))
            cz = cell_of(z(i))
            do dz = -1, 1
                do dy = -1, 1
                    do dx = -1, 1
                        neighbor = (modulo(cz + dz, cells)*cells + modulo(cy + dy, cells))*cells &
                                   + modulo(cx + dx, cells) + 1
                        j = head(neighbor)
                        do while (j /= 0)
                            if (j > i) then
                                rx = x(i) - x(j)
                                ry = y(i) - y(j)
                                rz = z(i) - z(j)
                                rx = rx - box*anint(rx/box)
                                ry = ry - box*anint(ry/box)
                                rz = rz - box*anint(rz/box)
                                r2 = rx*rx + ry*ry + rz*rz
                                if (r2 < cutoff*cutoff) then
                                    inverse_r2 = 1.0_real64/r2
                                    inverse_r6 = inverse_r2*inverse_r2*inverse_r2
                                    magnitude = 24.0_real64*inverse_r2*inverse_r6*(2.0_real64*inverse_r6 - 1.0_real64)
                                    thread_fx(i, t) = thread_fx(i, t) + magnitude*rx
                                    thread_fy(i, t) = thread_fy(i, t) + magnitude*ry
                                    thread_fz(i, t) = thread_fz(i, t) + magnitude*rz
                                    thread_fx(j, t) = thread_fx(j, t) - magnitude*rx
                                    thread_fy(j, t) = thread_fy(j, t) - magnitude*ry
                                    thread_fz(j, t) = thread_fz(j, t) - magnitude*rz
                                    partial = partial + 4.0_real64*inverse_r6*(inverse_r6 - 1.0_real64)
                                end if
                            end if
                            j = next(j)
                        end do
                    end do
                end do
            end do
        end do
        thread_energy(t) = partial
        !$omp barrier
        do i = first, last
            fx(i) = thread_fx(i, 1)
            fy(i) = thread_fy(i, 1)
            fz(i) = thread_fz(i, 1)
            do j = 2, threads
                fx(i) = fx(i) + thread_fx(i, j)
                fy(i) = fy(i) + thread_fy(i, j)
                fz(i) = fz(i) + thread_fz(i, j)
            end do
        end do
        !$omp end parallel

        energy = 0.0_real64
        do t = 1, threads
            energy = energy + thread_energy(t)
        end do
    end function compute_forces

    ! Kinetic energy, with every mass equal to one.
    function kinetic_energy() result(total)
        real(real64) :: total
        integer(int64) :: i

        total = 0.0_real64
        do i = 1, atoms
            total = total + 0.5_real64*(vx(i)*vx(i) + vy(i)*vy(i) + vz(i)*vz(i))
        end do
    end function kinetic_energy

    ! Builds the lattice, draws random velocities, removes net momentum and scales to `temperature`.
    subroutine initialize()
        real(real64), parameter :: basis(3, 4) = reshape([ &
                                                         0.0_real64, 0.0_real64, 0.0_real64, &
                                                         0.5_real64, 0.5_real64, 0.0_real64, &
                                                         0.5_real64, 0.0_real64, 0.5_real64, &
                                                         0.0_real64, 0.5_real64, 0.5_real64], [3, 4])
        type(random_state) :: random
        integer(int64) :: i, j, k, b, atom
        real(real64) :: spacing, mean_x, mean_y, mean_z, count, factor

        atoms = 4*lattice_cells**3
        box = (real(atoms, real64)/density)**(1.0_real64/3.0_real64)
        cells = int(box/cutoff, int64)
        allocate (x(atoms), y(atoms), z(atoms), vx(atoms), vy(atoms), vz(atoms))
        allocate (fx(atoms), fy(atoms), fz(atoms), head(cells**3), next(atoms))
        allocate (thread_fx(atoms, threads), thread_fy(atoms, threads), thread_fz(atoms, threads))
        allocate (thread_energy(threads))

        spacing = box/real(lattice_cells, real64)
        atom = 0
        do k = 0, lattice_cells - 1
            do j = 0, lattice_cells - 1
                do i = 0, lattice_cells - 1
                    do b = 1, 4
                        atom = atom + 1
                        x(atom) = (real(i, real64) + basis(1, b) + 0.25_real64)*spacing
                        y(atom) = (real(j, real64) + basis(2, b) + 0.25_real64)*spacing
                        z(atom) = (real(k, real64) + basis(3, b) + 0.25_real64)*spacing
                    end do
                end do
            end do
        end do

        mean_x = 0.0_real64
        mean_y = 0.0_real64
        mean_z = 0.0_real64
        do i = 1, atoms
            vx(i) = uniform_range(random, -0.5_real64, 0.5_real64)
            vy(i) = uniform_range(random, -0.5_real64, 0.5_real64)
            vz(i) = uniform_range(random, -0.5_real64, 0.5_real64)
            mean_x = mean_x + vx(i)
            mean_y = mean_y + vy(i)
            mean_z = mean_z + vz(i)
        end do
        count = real(atoms, real64)
        vx = vx - mean_x/count
        vy = vy - mean_y/count
        vz = vz - mean_z/count
        factor = sqrt(1.5_real64*count*temperature/kinetic_energy())
        vx = vx*factor
        vy = vy*factor
        vz = vz*factor
    end subroutine initialize

end program lennard_jones_half
