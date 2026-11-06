! Monte Carlo integration of exp(-|x|^2) over the 6-dimensional unit hypercube.
!
! Usage: monte_carlo <samples>
! Stresses: a serial random number stream, a transcendental function, a long reduction.
! Checksum: the estimate. The exact value is (sqrt(pi) / 2 * erf(1))^6 = 0.17350...;
! the error of the estimate shrinks as 1 / sqrt(samples).

program monte_carlo
    use support
    implicit none

    integer, parameter :: dimensions = 6

    integer(int64) :: samples, sample
    integer :: d
    real(real64) :: sum, radius_squared, coordinate, estimate, start, elapsed
    type(random_state) :: random

    samples = argument(1, '<samples>')

    start = seconds()
    sum = 0.0_real64
    do sample = 1, samples
        radius_squared = 0.0_real64
        do d = 1, dimensions
            coordinate = uniform(random)
            radius_squared = radius_squared + coordinate*coordinate
        end do
        sum = sum + exp(-radius_squared)
    end do
    estimate = sum/real(samples, real64)
    elapsed = seconds() - start

    call report(estimate, elapsed)

end program monte_carlo
