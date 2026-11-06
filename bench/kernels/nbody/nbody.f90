! N-body gravity: every body attracts every other, integrated with kick-drift-kick leapfrog.
!
! Usage: nbody <bodies> <steps>
! Stresses: an O(n^2) all-pairs loop with a reciprocal square root; vectorization of a reduction.
! Checksum: total energy (kinetic plus softened potential) after the last step.

program nbody
    use support
    implicit none

    real(real64), parameter :: time_step = 1.0e-3_real64
    real(real64), parameter :: softening_squared = 1.0e-2_real64
    character(len=*), parameter :: usage = '<bodies> <steps>'

    integer(int64) :: n, steps, step, i
    real(real64), allocatable :: x(:), y(:), z(:), vx(:), vy(:), vz(:), ax(:), ay(:), az(:), mass(:)
    type(random_state) :: random
    real(real64) :: start, elapsed

    n = argument(1, usage)
    steps = argument(2, usage)

    allocate (x(n), y(n), z(n), vx(n), vy(n), vz(n), ax(n), ay(n), az(n), mass(n))
    do i = 1, n
        x(i) = uniform_range(random, -1.0_real64, 1.0_real64)
        y(i) = uniform_range(random, -1.0_real64, 1.0_real64)
        z(i) = uniform_range(random, -1.0_real64, 1.0_real64)
        vx(i) = uniform_range(random, -0.05_real64, 0.05_real64)
        vy(i) = uniform_range(random, -0.05_real64, 0.05_real64)
        vz(i) = uniform_range(random, -0.05_real64, 0.05_real64)
        mass(i) = 1.0_real64/real(n, real64)
    end do

    start = seconds()
    call accelerate()
    do step = 1, steps
        vx = vx + 0.5_real64*time_step*ax
        vy = vy + 0.5_real64*time_step*ay
        vz = vz + 0.5_real64*time_step*az
        x = x + time_step*vx
        y = y + time_step*vy
        z = z + time_step*vz
        call accelerate()
        vx = vx + 0.5_real64*time_step*ax
        vy = vy + 0.5_real64*time_step*ay
        vz = vz + 0.5_real64*time_step*az
    end do
    elapsed = seconds() - start

    call report(energy(), elapsed)

contains

    ! Sets each body's acceleration from every body, itself included: its own term is zero thanks
    ! to the softening, and keeping it removes a branch from the inner loop.
    subroutine accelerate()
        integer(int64) :: i, j
        real(real64) :: dx, dy, dz, r2, inverse_r3, sum_x, sum_y, sum_z

        do i = 1, n
            sum_x = 0.0_real64
            sum_y = 0.0_real64
            sum_z = 0.0_real64
            do j = 1, n
                dx = x(j) - x(i)
                dy = y(j) - y(i)
                dz = z(j) - z(i)
                r2 = dx*dx + dy*dy + dz*dz + softening_squared
                inverse_r3 = mass(j)/(r2*sqrt(r2))
                sum_x = sum_x + dx*inverse_r3
                sum_y = sum_y + dy*inverse_r3
                sum_z = sum_z + dz*inverse_r3
            end do
            ax(i) = sum_x
            ay(i) = sum_y
            az(i) = sum_z
        end do
    end subroutine accelerate

    ! Kinetic plus potential energy, each distinct pair counted once.
    function energy() result(total)
        real(real64) :: total
        integer(int64) :: i, j
        real(real64) :: dx, dy, dz

        total = 0.0_real64
        do i = 1, n
            total = total + 0.5_real64*mass(i)*(vx(i)*vx(i) + vy(i)*vy(i) + vz(i)*vz(i))
            do j = i + 1, n
                dx = x(j) - x(i)
                dy = y(j) - y(i)
                dz = z(j) - z(i)
                total = total - mass(i)*mass(j)/sqrt(dx*dx + dy*dy + dz*dz + softening_squared)
            end do
        end do
    end function energy

end program nbody
