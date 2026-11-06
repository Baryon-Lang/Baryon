! Classical fourth-order Runge-Kutta on an ensemble of stiff Van der Pol oscillators,
!   x' = y,  y' = mu (1 - x^2) y - x,  with mu = 10,
! each starting from a different random amplitude.
!
! Usage: runge_kutta <oscillators> <steps>
! Stresses: many independent small systems; vectorization across the ensemble.
! Checksum: sum of x + y over the ensemble after the last step.
!
! mu = 10 makes the problem moderately stiff. The fixed step of 1e-3 sits well inside the explicit
! stability region; stiff implicit solvers are outside this suite.

program runge_kutta
    use support
    implicit none

    real(real64), parameter :: mu = 10.0_real64
    real(real64), parameter :: h = 1.0e-3_real64
    character(len=*), parameter :: usage = '<oscillators> <steps>'

    integer(int64) :: oscillators, steps, step, i
    real(real64), allocatable :: x(:), y(:)
    real(real64) :: x0, y0, kx1, ky1, kx2, ky2, kx3, ky3, kx4, ky4, checksum, start, elapsed
    type(random_state) :: random

    oscillators = argument(1, usage)
    steps = argument(2, usage)

    allocate (x(oscillators), y(oscillators))
    do i = 1, oscillators
        x(i) = uniform_range(random, 0.5_real64, 2.0_real64)
    end do
    y = 0.0_real64

    start = seconds()
    do step = 1, steps
        do i = 1, oscillators
            x0 = x(i)
            y0 = y(i)
            kx1 = y0
            ky1 = acceleration(x0, y0)
            kx2 = y0 + 0.5_real64*h*ky1
            ky2 = acceleration(x0 + 0.5_real64*h*kx1, y0 + 0.5_real64*h*ky1)
            kx3 = y0 + 0.5_real64*h*ky2
            ky3 = acceleration(x0 + 0.5_real64*h*kx2, y0 + 0.5_real64*h*ky2)
            kx4 = y0 + h*ky3
            ky4 = acceleration(x0 + h*kx3, y0 + h*ky3)
            x(i) = x0 + h/6.0_real64*(kx1 + 2.0_real64*kx2 + 2.0_real64*kx3 + kx4)
            y(i) = y0 + h/6.0_real64*(ky1 + 2.0_real64*ky2 + 2.0_real64*ky3 + ky4)
        end do
    end do
    elapsed = seconds() - start

    checksum = 0.0_real64
    do i = 1, oscillators
        checksum = checksum + (x(i) + y(i))
    end do
    call report(checksum, elapsed)

contains

    ! The right-hand side of the Van der Pol system for velocity component y.
    pure function acceleration(x, y) result(value)
        real(real64), intent(in) :: x, y
        real(real64) :: value

        value = mu*(1.0_real64 - x*x)*y - x
    end function acceleration

end program runge_kutta
