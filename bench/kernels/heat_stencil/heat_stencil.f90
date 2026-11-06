! 3D heat equation: explicit Jacobi time stepping with the 7-point Laplacian stencil on an n^3 grid.
!
! Usage: heat_stencil <points per side> <steps>
! Stresses: memory bandwidth; a regular stencil the compiler should vectorize; two-buffer swap.
! Checksum: sum of the temperature over the grid after the last step.
!
! The grid starts as sin(pi x) sin(pi y) sin(pi z) on the unit cube, with the boundary held at zero.
! The diffusion number alpha dt / h^2 is 1/8, inside the stability limit of 1/6.

program heat_stencil
    use support
    implicit none

    real(real64), parameter :: diffusion_number = 0.125_real64
    character(len=*), parameter :: usage = '<points per side> <steps>'

    integer(int64) :: n, steps, t, i, j, k
    real(real64), allocatable :: even(:, :, :), odd(:, :, :)
    real(real64) :: pi, h, start, elapsed

    n = argument(1, usage)
    steps = argument(2, usage)

    pi = acos(-1.0_real64)
    h = 1.0_real64/real(n - 1, real64)
    allocate (even(0:n - 1, 0:n - 1, 0:n - 1))
    even = 0.0_real64
    do k = 1, n - 2
        do j = 1, n - 2
            do i = 1, n - 2
                even(i, j, k) = sin(pi*real(i, real64)*h)*sin(pi*real(j, real64)*h)*sin(pi*real(k, real64)*h)
            end do
        end do
    end do
    odd = even

    ! Even steps read `even` and write `odd`; odd steps the reverse. No data is copied.
    start = seconds()
    do t = 0, steps - 1
        if (modulo(t, 2_int64) == 0) then
            call step(even, odd)
        else
            call step(odd, even)
        end if
    end do
    elapsed = seconds() - start

    if (modulo(steps, 2_int64) == 0) then
        call report(sum(even), elapsed)
    else
        call report(sum(odd), elapsed)
    end if

contains

    ! One time step from `from` into `to`. Boundary points are never written, so they stay zero.
    subroutine step(from, to)
        real(real64), intent(in) :: from(0:, 0:, 0:)
        real(real64), intent(inout) :: to(0:, 0:, 0:)
        integer(int64) :: i, j, k

        do k = 1, n - 2
            do j = 1, n - 2
                do i = 1, n - 2
                    to(i, j, k) = from(i, j, k) + diffusion_number*(from(i - 1, j, k) + from(i + 1, j, k) &
                                                                    + from(i, j - 1, k) + from(i, j + 1, k) &
                                                                    + from(i, j, k - 1) + from(i, j, k + 1) &
                                                                    - 6.0_real64*from(i, j, k))
                end do
            end do
        end do
    end subroutine step

end program heat_stencil
