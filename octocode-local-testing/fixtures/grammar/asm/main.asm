section .text
global run
helper:
    mov eax, edi
    ret
run:
    call helper
    ret
