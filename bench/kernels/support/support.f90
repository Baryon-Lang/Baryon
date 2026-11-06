! Shared support for the Fortran kernels: argument parsing, the random number generator, the timer
! and the report every kernel prints. `support.hpp` is the C++ twin of this file; the two must stay
! identical in behavior, because the harness checks that both languages agree.

module support
    use, intrinsic :: iso_fortran_env, only: int64, real64, error_unit
    implicit none
    private

    public :: int64, real64
    public :: argument, random_state, uniform, uniform_range, seconds, report

    ! The fixed seed every kernel starts from, so every run sees the same input data.
    integer(int64), parameter :: seed = 88172645463325252_int64

    ! Marsaglia's xorshift64 generator with shifts 13, 7, 17. It uses only shifts and exclusive-or,
    ! which Fortran defines for signed 64-bit integers, so both languages produce the same stream.
    type :: random_state
        integer(int64) :: state = seed
    end type random_state

contains

    ! Returns command-line argument `position` (counted from 1) as an integer. Stops with status 2
    ! and a usage message if the argument is missing or is not a positive integer.
    function argument(position, usage) result(value)
        integer, intent(in) :: position
        character(len=*), intent(in) :: usage
        integer(int64) :: value
        character(len=64) :: text
        integer :: length, status

        call get_command_argument(position, text, length, status)
        if (status /= 0 .or. length == 0) call usage_error(usage)
        read (text, *, iostat=status) value
        if (status /= 0 .or. value <= 0) call usage_error(usage)
    end function argument

    ! Prints the usage message and stops with status 2.
    subroutine usage_error(usage)
        character(len=*), intent(in) :: usage
        character(len=256) :: program

        call get_command_argument(0, program)
        write (error_unit, '(a)') 'usage: '//trim(program)//' '//usage
        error stop 2
    end subroutine usage_error

    ! Returns a uniform double in [0, 1) built from the top 53 bits of the next state.
    function uniform(random) result(value)
        type(random_state), intent(inout) :: random
        real(real64) :: value

        random%state = ieor(random%state, ishft(random%state, 13))
        random%state = ieor(random%state, ishft(random%state, -7))
        random%state = ieor(random%state, ishft(random%state, 17))
        value = real(ishft(random%state, -11), real64)*scale(1.0_real64, -53)
    end function uniform

    ! Returns a uniform double in [low, high).
    function uniform_range(random, low, high) result(value)
        type(random_state), intent(inout) :: random
        real(real64), intent(in) :: low, high
        real(real64) :: value

        value = low + (high - low)*uniform(random)
    end function uniform_range

    ! Seconds since an arbitrary fixed point, from a monotonic clock.
    function seconds() result(value)
        real(real64) :: value
        integer(int64) :: count, rate

        call system_clock(count, rate)
        value = real(count, real64)/real(rate, real64)
    end function seconds

    ! Prints the two lines the harness reads: the checksum that proves the result, and the time the
    ! timed region took, in seconds.
    subroutine report(checksum, elapsed)
        real(real64), intent(in) :: checksum, elapsed

        write (*, '(a, es25.17e3)') 'checksum ', checksum
        write (*, '(a, es25.17e3)') 'seconds ', elapsed
    end subroutine report

end module support
