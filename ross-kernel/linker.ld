ENTRY(_start)

SECTIONS
{
    /* Link the kernel in the higher half */
    . = 0xFFFFFFFF80200000;

    .text : AT(0x200000)
    {
        *(.text._start)
        *(.text*)
    }

    .rodata : AT(ADDR(.rodata) - 0xFFFFFFFF80000000)
    {
        *(.rodata*)
    }

    .data : AT(ADDR(.data) - 0xFFFFFFFF80000000)
    {
        *(.data*)
    }

    .bss : AT(ADDR(.bss) - 0xFFFFFFFF80000000)
    {
        *(.bss*)
    }

    _bss_start = ADDR(.bss);
    _bss_end = ADDR(.bss) + SIZEOF(.bss);
}
