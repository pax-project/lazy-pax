# Build — single responsibility: the crate actually compiles. Distinct
# from test.just, which proves behavior rather than mere compilation.
#
# [no-cd]: see just/fmt.just.

[no-cd]
check:
    cargo build
