! Fast Fourier transform: iterative radix-2 Cooley-Tukey on 2^p complex points, repeated.
!
! Usage: fft <log2 of points> <repetitions>
! Stresses: strided access that changes every pass, a bit-reversal permutation, complex arithmetic.
! Checksum: sum of the magnitudes of the transform from the last repetition.
!
! Complex numbers are stored as separate real and imaginary arrays and multiplied by hand, so both
! languages do exactly the same floating-point operations. Twiddle factors are computed once,
! outside the timed region, as a real FFT library would. Arrays are indexed from 0 so the index
! arithmetic matches the C++ version line for line.

program fft
    use support
    implicit none

    character(len=*), parameter :: usage = '<log2 of points> <repetitions>'

    integer(int64) :: log2_points, repetitions, repetition, n, i, j, k, bit, length, half, stride
    integer(int64) :: block, top, bottom
    real(real64), allocatable :: input_re(:), input_im(:), re(:), im(:), twiddle_re(:), twiddle_im(:)
    real(real64) :: pi, angle, w_re, w_im, t_re, t_im, swap, checksum, start, elapsed
    type(random_state) :: random

    log2_points = argument(1, usage)
    repetitions = argument(2, usage)
    n = shiftl(1_int64, int(log2_points))

    allocate (input_re(0:n - 1), input_im(0:n - 1), re(0:n - 1), im(0:n - 1))
    allocate (twiddle_re(0:n/2 - 1), twiddle_im(0:n/2 - 1))
    do i = 0, n - 1
        input_re(i) = uniform_range(random, -1.0_real64, 1.0_real64)
        input_im(i) = uniform_range(random, -1.0_real64, 1.0_real64)
    end do
    pi = acos(-1.0_real64)
    do k = 0, n/2 - 1
        angle = -2.0_real64*pi*real(k, real64)/real(n, real64)
        twiddle_re(k) = cos(angle)
        twiddle_im(k) = sin(angle)
    end do

    start = seconds()
    do repetition = 1, repetitions
        re = input_re
        im = input_im
        ! Bit-reversal permutation.
        j = 0
        do i = 1, n - 1
            bit = shiftr(n, 1)
            do while (iand(j, bit) /= 0)
                j = ieor(j, bit)
                bit = shiftr(bit, 1)
            end do
            j = ieor(j, bit)
            if (i < j) then
                swap = re(i)
                re(i) = re(j)
                re(j) = swap
                swap = im(i)
                im(i) = im(j)
                im(j) = swap
            end if
        end do
        ! Butterfly passes over blocks of length 2, 4, ..., n.
        length = 2
        do while (length <= n)
            half = shiftr(length, 1)
            stride = n/length
            do block = 0, n - 1, length
                do k = 0, half - 1
                    w_re = twiddle_re(k*stride)
                    w_im = twiddle_im(k*stride)
                    top = block + k
                    bottom = top + half
                    t_re = w_re*re(bottom) - w_im*im(bottom)
                    t_im = w_re*im(bottom) + w_im*re(bottom)
                    re(bottom) = re(top) - t_re
                    im(bottom) = im(top) - t_im
                    re(top) = re(top) + t_re
                    im(top) = im(top) + t_im
                end do
            end do
            length = shiftl(length, 1)
        end do
    end do
    elapsed = seconds() - start

    checksum = 0.0_real64
    do i = 0, n - 1
        checksum = checksum + sqrt(re(i)*re(i) + im(i)*im(i))
    end do
    call report(checksum, elapsed)

end program fft
