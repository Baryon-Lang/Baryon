! Conjugate gradient on a sparse matrix: the 3D Poisson problem -laplace(u) = 1 on an n^3 grid of
! unknowns with zero Dirichlet boundaries, stored in compressed sparse row (CSR) form.
!
! Usage: conjugate_gradient <points per side> <iterations>
!   Runs exactly <iterations> iterations with no early exit, so the work is fixed.
! Stresses: sparse matrix-vector products with indirect column access; dot-product reductions.
! Checksum: Euclidean norm of the solution after the last iteration.

program conjugate_gradient
    use support
    implicit none

    character(len=*), parameter :: usage = '<points per side> <iterations>'

    integer(int64) :: n, iterations, iteration, unknowns
    ! Row r holds entries offsets(r) .. offsets(r + 1) - 1.
    integer(int64), allocatable :: offsets(:), columns(:)
    ! Entries stored so far while the matrix is built.
    integer(int64) :: entries
    real(real64), allocatable :: values(:), x(:), r(:), p(:), q(:)
    real(real64) :: rho, rho_next, alpha, beta, start, elapsed

    n = argument(1, usage)
    iterations = argument(2, usage)

    unknowns = n**3
    call build_laplacian()
    allocate (x(unknowns), r(unknowns), p(unknowns), q(unknowns))
    x = 0.0_real64
    r = 1.0_real64
    p = 1.0_real64
    q = 0.0_real64

    start = seconds()
    rho = dot(r, r)
    do iteration = 1, iterations
        call multiply(p, q)
        alpha = rho/dot(p, q)
        x = x + alpha*p
        r = r - alpha*q
        rho_next = dot(r, r)
        beta = rho_next/rho
        rho = rho_next
        p = r + beta*p
    end do
    elapsed = seconds() - start

    call report(sqrt(dot(x, x)), elapsed)

contains

    ! Builds the 7-point Laplacian (6 on the diagonal, -1 per grid neighbor) with columns in
    ! increasing order. Unknown (i, j, k), counted from 0, is row (k * n + j) * n + i + 1.
    subroutine build_laplacian()
        integer(int64) :: i, j, k, row

        allocate (offsets(unknowns + 1), columns(7*unknowns), values(7*unknowns))
        entries = 0
        offsets(1) = 1
        do k = 0, n - 1
            do j = 0, n - 1
                do i = 0, n - 1
                    row = (k*n + j)*n + i + 1
                    if (k > 0) call add(row - n*n, -1.0_real64)
                    if (j > 0) call add(row - n, -1.0_real64)
                    if (i > 0) call add(row - 1, -1.0_real64)
                    call add(row, 6.0_real64)
                    if (i < n - 1) call add(row + 1, -1.0_real64)
                    if (j < n - 1) call add(row + n, -1.0_real64)
                    if (k < n - 1) call add(row + n*n, -1.0_real64)
                    offsets(row + 1) = entries + 1
                end do
            end do
        end do

    end subroutine build_laplacian

    ! Appends one entry to the row being built.
    subroutine add(column, value)
        integer(int64), intent(in) :: column
        real(real64), intent(in) :: value

        entries = entries + 1
        columns(entries) = column
        values(entries) = value
    end subroutine add

    ! y = A x.
    subroutine multiply(x, y)
        real(real64), intent(in) :: x(:)
        real(real64), intent(out) :: y(:)
        integer(int64) :: row, e
        real(real64) :: total

        do row = 1, unknowns
            total = 0.0_real64
            do e = offsets(row), offsets(row + 1) - 1
                total = total + values(e)*x(columns(e))
            end do
            y(row) = total
        end do
    end subroutine multiply

    ! The dot product, summed in index order like the C++ version.
    function dot(u, v) result(total)
        real(real64), intent(in) :: u(:), v(:)
        real(real64) :: total
        integer(int64) :: i

        total = 0.0_real64
        do i = 1, size(u, kind=int64)
            total = total + u(i)*v(i)
        end do
    end function dot

end program conjugate_gradient
