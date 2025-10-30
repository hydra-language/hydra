.section .note.GNU-stack,"",%progbits
.text

.global _start
.type _start, @function

_start:
    # Linux x86_64 System V ABI: set up argc/argv/envp for main
    xor    %rbp, %rbp
    mov    %rsp, %rdi           # rdi = argc (as pointer to argc) - some programs expect argc, argv; we'll pass argc as int via rdi below
    # Actually get argc and argv:
    mov    (%rsp), %edi         # edi = argc (int)
    lea    8(%rsp), %rsi        # rsi = &argv

    # Call main(argc, argv)
    call   main

    # exit(return_code)
    mov    %eax, %edi           # exit code -> edi
    mov    $60, %eax            # syscall: exit
    syscall

