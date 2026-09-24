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

    mov     x0, #0
    mov     x8, #93
    svc     #0

