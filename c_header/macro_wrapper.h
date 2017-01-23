#define _GNU_SOURCE
#include <sched.h>
#undef _GNU_SOURCE

int macro_CPU_EQUAL(cpu_set_t * set1, cpu_set_t * set2);

void macro_CPU_ZERO(cpu_set_t * set);

void macro_CPU_SET(int cpu, cpu_set_t * set);

void macro_CPU_CLR(int cpu, cpu_set_t * set);

int macro_CPU_ISSET(int cpu, cpu_set_t * set);

int macro_CPU_COUNT(cpu_set_t * set);

void macro_CPU_AND(cpu_set_t * destset, cpu_set_t * srcset1, cpu_set_t * srcset2);

void macro_CPU_OR(cpu_set_t * destset, cpu_set_t * srcset1, cpu_set_t * srcset2);

void macro_CPU_XOR(cpu_set_t * destset, cpu_set_t * srcset1, cpu_set_t * srcset2);