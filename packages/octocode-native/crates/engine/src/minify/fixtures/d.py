#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Module docstring."""
import os  # noqa: F401

# comment between statements
x = 1
# another comment
y = "# not comment"
SQL = """
SELECT a:
"""
print("kept after SQL")
Z = """z"""

def f():
    """Example:
    x = 1
    return x
    """
    x = 1
    return x

d = {
    "key":
        """value string that matters""",
}
r = r"""a " # kept?"""
s = 'it\'s # x'

class C:
    def m(self):  # type: ignore
        '''doc'''

        return 1
