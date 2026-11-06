! Lennard-Jones molecular dynamics: a liquid of argon-like atoms in a periodic box, integrated with
! velocity Verlet. Forces come from a cell list rebuilt every step.
!
! Usage: lennard_jones <cells per side> <steps>
!   The crystal starts as a face-centred cubic lattice of 4 * cells^3 atoms. Use 5 or more cells
!   so the force cell grid is at least 3 wide.
! Stresses: indirect, data-dependent memory access; a cutoff branch; minimum-image rounding.
! Checksum: total energy per atom after the last step, in reduced units.

program lennard_jones
    use support
    use, intrinsic :: iso_fortran_env, only: error_unit
    implicit none

    real(real64), parameter :: density = 0.8442_real64
    real(real64), parameter :: temperature = 0.72_real64
    real(real64), parameter :: cutoff = 2.5_real64
    real(real64), parameter :: time_step = 0.005_real64
    character(len=*), parameter :: usage = '<cells per side> <steps>'

    integer(int64) :: lattice_cells, steps, step, atoms, cells
    real(real64) :: box, potential, start, elapsed
    real(real64), allocatable :: x(:), y(:), z(:), vx(:), vy(:), vz(:), fx(:), fy(:), fz(:)
    ! Linked list of atoms per force cell; 0 ends a list.
    integer(int64), allocatable :: head(:), next(:)

    lattice_cells = argument(1, usage)
    steps = argument(2, usage)

    call initialize()
    if (cells < 3) then
        write (error_unit, '(a)') 'lennard_jones: use at least 5 cells per side'
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

    ! Sets every force and returns the potential energy. Each pair is visited from both sides, so
    ! each atom only writes its own force and half of each pair energy is counted.
    function compute_forces() result(energy)
        real(real64) :: energy
        integer(int64) :: i, j, cell, neighbor, cx, cy, cz, dx, dy, dz
        real(real64) :: rx, ry, rz, r2, inverse_r2, inverse_r6, magnitude, sum_x, sum_y, sum_z

        head = 0
        do i = 1, atoms
            cell = (cell_of(z(i))*cells + cell_of(y(i)))*cells + cell_of(x(i)) + 1
            next(i) = head(cell)
            head(cell) = i
        end do

        energy = 0.0_real64
        do i = 1, atoms
            cx = cell_of(x(i))
            cy = cell_of(y(i))
            cz = cell_of(z(i))
            sum_x = 0.0_real64
            sum_y = 0.0_real64
            sum_z = 0.0_real64
            do dz = -1, 1
                do dy = -1, 1
                    do dx = -1, 1
                        neighbor = (modulo(cz + dz, cells)*cells + modulo(cy + dy, cells))*cells &
                                   + modulo(cx + dx, cells) + 1
                        j = head(neighbor)
                        do while (j /= 0)
                            if (j /= i) then
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
                                    sum_x = sum_x + magnitude*rx
                                    sum_y = sum_y + magnitude*ry
                                    sum_z = sum_z + magnitude*rz
                                    energy = energy + 2.0_real64*inverse_r6*(inverse_r6 - 1.0_real64)
                                end if
                            end if
                            j = next(j)
                        end do
                    end do
                end do
            end do
            fx(i) = sum_x
            fy(i) = sum_y
            fz(i) = sum_z
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

end program lennard_jones
