#!/bin/bash

generate_output() {
    local line=1
    while sleep 1
    do
        echo "this is line $line"
        line=$((line + 1))
    done
}

generate_error() {
    local line=1
    while sleep 4
    do
        echo "this is error line $line"
        line=$((line + 1))
    done
}

generate_progress() {
    local prog=1
    local extra=""
    while sleep 0.1
    do
        printf "progress step %d\n$extra\f" $prog
        prog=$((prog + 1))
        if [[ $prog = "100" ]]
        then
            extra="\033[32mprogress can have colors and multiple lines\n"
        fi
    done
}

# Calling processes can provide input file descriptors using environment variables:
generate_output | PAGER_TITLE=generate_output PAGER_ERROR_FD=55 PAGER_PROGRESS_FD=66 cargo run 55< <(generate_error) 66< <(generate_progress)

# Calling processes can provide input file descriptors using arguments:
# generate_output | cargo run -- 55< <(generate_error) 66< <(generate_progress) --fd 0=generate_output --error-fd 55=ERR --progress-fd 66

