.section .note.GNU-stack,"",%progbits
.text
    
.global _start
.type _start, %function

_start:
    // r0 = argc, r1 = argv (AArch64 calling convention)
    mov     x0, sp            // x0 points to argc on the stack
    ldr     w0, [sp]          // w0 = argc (first 4 bytes of stack)
    add     x1, sp, #8        // x1 = &argv (argv starts just after argc)

    // Call main(argc, argv)
    bl      main

    // exit(return_code)
    mov     x1, x0            // move return code from main to x0
    mov     x8, #93           // syscall number for exit (64-bit)
    svc     #0                // make syscall

