.section .note.GNU-stack,"",%progbits

.section .data
str_true:   .ascii "true"
str_false:  .ascii "false"
str_minus:  .ascii "-"
str_nl:     .ascii "\n"

.text

.global _start
.global print_str

# ---------------------------------------------------------
# Entry Point
# ---------------------------------------------------------
_start:
    # Linux x86_64 System V ABI: set up argc/argv/envp for main
    xor    %rbp, %rbp
    mov    %rsp, %rdi           # rdi = argc (as pointer to argc)
    mov    (%rsp), %edi         # edi = argc (int)
    lea    8(%rsp), %rsi        # rsi = &argv

    # Call main(argc, argv)
    call   main

    # Exit using Linux sys_exit (syscall 60)
    mov    %eax, %edi           # exit code -> edi
    mov    $60, %rax            # sys_exit
    syscall

# ---------------------------------------------------------
# print_str(ptr: *u8, len: u64)
# ---------------------------------------------------------
print_str:
    # Arguments: rdi = ptr, rsi = len
    mov    %rsi, %rdx           # rdx = length
    mov    %rdi, %rsi           # rsi = pointer to string
    mov    $1, %rdi             # fd = 1 (stdout)
    mov    $1, %rax             # sys_write (syscall 1)
    syscall
    ret
