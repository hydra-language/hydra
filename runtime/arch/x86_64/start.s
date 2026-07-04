.section .note.GNU-stack,"",%progbits

.section .data
str_true:   .ascii "true"
str_false:  .ascii "false"
str_minus:  .ascii "-"
str_nl:     .ascii "\n"

.text

.global _start
.global print_str
.global print_u64
.global print_i64
.global print_bool
.global print_newline

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

# ---------------------------------------------------------
# print_u64(val: u64)
# ---------------------------------------------------------
print_u64:
    # Arguments: rdi = unsigned 64-bit integer
    mov    %rdi, %rax           # rax = number to format
    mov    $10, %rcx            # rcx = divisor
    sub    $32, %rsp            # allocate 32 bytes on stack for string buffer
    lea    31(%rsp), %rsi       # rsi points to the end of the buffer

.L_u64_loop:
    dec    %rsi
    xor    %rdx, %rdx           # clear rdx for div
    div    %rcx                 # rdx:rax / 10 -> rax = quotient, rdx = remainder
    add    $48, %dl             # convert remainder to ascii ('0' + rdx)
    mov    %dl, (%rsi)          # store character in buffer
    test   %rax, %rax           # is quotient 0?
    jnz    .L_u64_loop          # if not, keep looping

    # String is generated backwards. rsi now points to the start of the string.
    lea    31(%rsp), %rdx       # rdx = address of end of buffer
    sub    %rsi, %rdx           # rdx = length (end - start)

    mov    $1, %rdi             # fd = 1 (stdout)
    mov    $1, %rax             # sys_write (syscall 1)
    syscall

    add    $32, %rsp            # clean up stack
    ret

# ---------------------------------------------------------
# print_i64(val: i64)
# ---------------------------------------------------------
print_i64:
    # Arguments: rdi = signed 64-bit integer
    test   %rdi, %rdi
    jns    print_u64            # If positive, just jump to print_u64

    # If negative, print a minus sign first
    push   %rdi                 # Save the number
    sub    $8, %rsp             # Align stack to 16 bytes

    mov    $1, %rdi             # fd = 1 (stdout)
    lea    str_minus(%rip), %rsi
    mov    $1, %rdx             # length = 1
    mov    $1, %rax             # sys_write (syscall 1)
    syscall

    add    $8, %rsp             # Restore alignment
    pop    %rdi                 # Restore number
    neg    %rdi                 # Make it positive
    jmp    print_u64            # Jump to u64 logic to print the digits

# ---------------------------------------------------------
# print_bool(val: bool)
# ---------------------------------------------------------
print_bool:
    # Arguments: rdi = 0 or 1
    test   %rdi, %rdi
    jz     .L_false

    lea    str_true(%rip), %rsi
    mov    $4, %rdx             # length = 4
    jmp    .L_print_bool_exec

.L_false:
    lea    str_false(%rip), %rsi
    mov    $5, %rdx             # length = 5

.L_print_bool_exec:
    mov    $1, %rdi             # fd = 1 (stdout)
    mov    $1, %rax             # sys_write (syscall 1)
    syscall
    ret

# ---------------------------------------------------------
# print_newline()
# ---------------------------------------------------------
print_newline:
    mov    $1, %rdi             # fd = 1 (stdout)
    lea    str_nl(%rip), %rsi
    mov    $1, %rdx             # length = 1
    mov    $1, %rax             # sys_write (syscall 1)
    syscall
    ret
