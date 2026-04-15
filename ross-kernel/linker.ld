ENTRY(_start)

SECTIONS
{
    /* Start at 2MB */
    . = 0xFFFFFFFF80200000; 

    /* Code section */
    .text :
    {
        /* Force the entry point to be at the very first byte of the file */
        KEEP(*(.text._start))
        *(.text .text.*)
    }

    /* Read-only data (like string literals) */
    .rodata :
    {
        *(.rodata .rodata.*)
    }

    /* Initialized data */
    .data :
    {
        *(.data .data.*)
    }

    /* Uninitialized data (zero-filled) */
    .bss :
    {
        _bss_start = .;
        *(.bss .bss.*)
        _bss_end = .;
    }

    /* Define the end of the kernel for memory management later */
    _kernel_end = .;
}

