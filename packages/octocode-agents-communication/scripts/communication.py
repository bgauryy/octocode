#!/usr/bin/env python3
"""Portable entry point; run with Python 3.9 or newer. No build or pip install."""
import sys

sys.dont_write_bytecode = True
if sys.version_info < (3, 9):
    sys.exit('Python 3.9 or newer is required for agents communication.')

from communication.cli import main

if __name__ == '__main__':
    main()
