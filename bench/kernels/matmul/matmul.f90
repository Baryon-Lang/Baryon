! Dense matrix multiplication C = A B for n x n matrices, written as plain loops.
!
! Usage: matmul <n>
! Stresses: floating-point throughput, cache reuse and vectorization of the innermost loop.
! Checksum: sum of every element of C.
!
! No BLAS and no `matmul` intrinsic: the suite measures what the compiler does with straightforward
! code. Element (i, j) of A and B is drawn row by row in both languages, so both multiply the same
! matrices. The loop order j, k, i keeps the inner loop contiguous in column-major storage, the
! mirror of the C++ order.

program matmul_kernel
    use support
    implicit none

    integer(int64) :: n, i, j, k
    real(real64), allocatable :: a(:, :), b(:, :), c(:, :)
    real(real64) :: b_kj, checksum, start, elapsed
    type(random_state) :: random

    n = argument(1, '<n>')

    allocate (a(n, n), b(n, n), c(n, n))
    do i = 1, n
        do j = 1, n
            a(i, j) = uniform_range(random, -1.0_real64, 1.0_real64)
        end do
    end do
    do i = 1, n
        do j = 1, n
            b(i, j) = uniform_range(random, -1.0_real64, 1.0_real64)
        end do
    end do
    c = 0.0_real64

    start = seconds()
    do j = 1, n
        do k = 1, n
            b_kj = b(k, j)
            do i = 1, n
                c(i, j) = c(i, j) + a(i, k)*b_kj
            end do
        end do
    end do
    elapsed = seconds() - start

    ! Summed row by row, the same order as the C++ version.
    checksum = 0.0_real64
    do i = 1, n
        do j = 1, n
            checksum = checksum + c(i, j)
        end do
    end do
    call report(checksum, elapsed)

end program matmul_kernel
