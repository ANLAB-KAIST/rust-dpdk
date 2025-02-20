#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <sys/types.h>
#include <inttypes.h>
#include "dpdk.h"

void __attribute__ ((noinline)) test_fn() 
{
    void* __unused = __CHECK_FN;
}

int main() {
    test_fn();
    return 0;
}
