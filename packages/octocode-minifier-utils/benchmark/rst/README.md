# reStructuredText (.rst)

Source sample: `rst/cpython-tutorial-index.rst`

Strategy: `conservative`

Agent rating: **6.3/10 (fair)**

Agent understanding from minified output: **8/10 (strong)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 2616 | - | - | - |
| content-view | 2569 | 1.8% | 0.231 ms | 6.3/10 |
| applyMinification | 2571 | 1.7% | 0.206 ms | 6.3/10 |
| sync minify | 2571 | 1.7% | 0.203 ms | 6.3/10 |
| async minify | 2571 | 1.7% | 0.211 ms | 6.3/10 |
| symbols | 2707 | -3.5% | 0.114 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 6.7/10 (2/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 6/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 2616 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 2569 | 1.8% | 8/10 strong | 6.7/10 | 10/10 |
| minify | 2571 | 1.7% | 8/10 strong | 6.7/10 | 10/10 |
| symbols | 2707 | -3.5% | 6.3/10 fair | 3.3/10 | 10/10 |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```rst
.. _tutorial-index:

######################
  The Python Tutorial
######################

.. Tip:: This tutorial is designed for
   *programmers* that are new to the Python language,
   **not** *beginners* who are new to programming.

Python is an easy to learn, powerful programming language. It has efficient
high-level data structures and a simple but effective approach to
object-oriented programming. Python's elegant syntax and dynamic typing,
together with its interpreted nature, make it an ideal language for scripting
and rapid application development in many areas on most platforms.

The Python interpreter and the extensive standard library are freely available
in source or binary form for all major platforms from the Python website,
https://www.python.org/, and may be freely distributed. The same site also
contains distributions of and pointers to many free third party Python modules,
programs and tools, and additional documentation.

The Python interpreter is easily extended with new functions and data types
implemented in C or C++ (or other languages callable from C). Python is also
suitable as an extension language for customizable applications.

This tutorial introduces the reader informally to 

... [truncated 816 chars] ...

he
language's flavor and style. After reading it, you will be able to read and
write Python modules and programs, and you will be ready to learn more about the
various Python library modules described in :ref:`library-index`.

The :ref:`glossary` is also worth going through.

.. toctree::
   :numbered:

   appetite.rst
   interpreter.rst
   introduction.rst
   controlflow.rst
   datastructures.rst
   modules.rst
   inputoutput.rst
   errors.rst
   classes.rst
   stdlib.rst
   stdlib2.rst
   venv.rst
   whatnow.rst
   interactive.rst
   floatingpoint.rst
   appendix.rst

```

## Content-View Excerpt

```rst
.. _tutorial-index:

  The Python Tutorial

.. Tip:: This tutorial is designed for
   *programmers* that are new to the Python language,
   **not** *beginners* who are new to programming.

Python is an easy to learn, powerful programming language. It has efficient
high-level data structures and a simple but effective approach to
object-oriented programming. Python's elegant syntax and dynamic typing,
together with its interpreted nature, make it an ideal language for scripting
and rapid application development in many areas on most platforms.

The Python interpreter and the extensive standard library are freely available
in source or binary form for all major platforms from the Python website,
https://www.python.org/, and may be freely distributed. The same site also
contains distributions of and pointers to many free third party Python modules,
programs and tools, and additional documentation.

The Python interpreter is easily extended with new functions and data types
implemented in C or C++ (or other languages callable from C). Python is also
suitable as an extension language for customizable applications.

This tutorial introduces the reader informally to the basic concepts and
features of the Python 

... [truncated 769 chars] ...

the
language's flavor and style. After reading it, you will be able to read and
write Python modules and programs, and you will be ready to learn more about the
various Python library modules described in :ref:`library-index`.

The :ref:`glossary` is also worth going through.

.. toctree::
   :numbered:

   appetite.rst
   interpreter.rst
   introduction.rst
   controlflow.rst
   datastructures.rst
   modules.rst
   inputoutput.rst
   errors.rst
   classes.rst
   stdlib.rst
   stdlib2.rst
   venv.rst
   whatnow.rst
   interactive.rst
   floatingpoint.rst
   appendix.rst
```

## Apply Minification Excerpt

```rst
.. _tutorial-index:


  The Python Tutorial


.. Tip:: This tutorial is designed for
   *programmers* that are new to the Python language,
   **not** *beginners* who are new to programming.

Python is an easy to learn, powerful programming language. It has efficient
high-level data structures and a simple but effective approach to
object-oriented programming. Python's elegant syntax and dynamic typing,
together with its interpreted nature, make it an ideal language for scripting
and rapid application development in many areas on most platforms.

The Python interpreter and the extensive standard library are freely available
in source or binary form for all major platforms from the Python website,
https://www.python.org/, and may be freely distributed. The same site also
contains distributions of and pointers to many free third party Python modules,
programs and tools, and additional documentation.

The Python interpreter is easily extended with new functions and data types
implemented in C or C++ (or other languages callable from C). Python is also
suitable as an extension language for customizable applications.

This tutorial introduces the reader informally to the basic concepts and
features of the Pytho

... [truncated 771 chars] ...

the
language's flavor and style. After reading it, you will be able to read and
write Python modules and programs, and you will be ready to learn more about the
various Python library modules described in :ref:`library-index`.

The :ref:`glossary` is also worth going through.

.. toctree::
   :numbered:

   appetite.rst
   interpreter.rst
   introduction.rst
   controlflow.rst
   datastructures.rst
   modules.rst
   inputoutput.rst
   errors.rst
   classes.rst
   stdlib.rst
   stdlib2.rst
   venv.rst
   whatnow.rst
   interactive.rst
   floatingpoint.rst
   appendix.rst
```

## Sync Minify Excerpt

```rst
.. _tutorial-index:


  The Python Tutorial


.. Tip:: This tutorial is designed for
   *programmers* that are new to the Python language,
   **not** *beginners* who are new to programming.

Python is an easy to learn, powerful programming language. It has efficient
high-level data structures and a simple but effective approach to
object-oriented programming. Python's elegant syntax and dynamic typing,
together with its interpreted nature, make it an ideal language for scripting
and rapid application development in many areas on most platforms.

The Python interpreter and the extensive standard library are freely available
in source or binary form for all major platforms from the Python website,
https://www.python.org/, and may be freely distributed. The same site also
contains distributions of and pointers to many free third party Python modules,
programs and tools, and additional documentation.

The Python interpreter is easily extended with new functions and data types
implemented in C or C++ (or other languages callable from C). Python is also
suitable as an extension language for customizable applications.

This tutorial introduces the reader informally to the basic concepts and
features of the Pytho

... [truncated 771 chars] ...

the
language's flavor and style. After reading it, you will be able to read and
write Python modules and programs, and you will be ready to learn more about the
various Python library modules described in :ref:`library-index`.

The :ref:`glossary` is also worth going through.

.. toctree::
   :numbered:

   appetite.rst
   interpreter.rst
   introduction.rst
   controlflow.rst
   datastructures.rst
   modules.rst
   inputoutput.rst
   errors.rst
   classes.rst
   stdlib.rst
   stdlib2.rst
   venv.rst
   whatnow.rst
   interactive.rst
   floatingpoint.rst
   appendix.rst
```

## Async Minify Excerpt

```rst
.. _tutorial-index:


  The Python Tutorial


.. Tip:: This tutorial is designed for
   *programmers* that are new to the Python language,
   **not** *beginners* who are new to programming.

Python is an easy to learn, powerful programming language. It has efficient
high-level data structures and a simple but effective approach to
object-oriented programming. Python's elegant syntax and dynamic typing,
together with its interpreted nature, make it an ideal language for scripting
and rapid application development in many areas on most platforms.

The Python interpreter and the extensive standard library are freely available
in source or binary form for all major platforms from the Python website,
https://www.python.org/, and may be freely distributed. The same site also
contains distributions of and pointers to many free third party Python modules,
programs and tools, and additional documentation.

The Python interpreter is easily extended with new functions and data types
implemented in C or C++ (or other languages callable from C). Python is also
suitable as an extension language for customizable applications.

This tutorial introduces the reader informally to the basic concepts and
features of the Pytho

... [truncated 771 chars] ...

the
language's flavor and style. After reading it, you will be able to read and
write Python modules and programs, and you will be ready to learn more about the
various Python library modules described in :ref:`library-index`.

The :ref:`glossary` is also worth going through.

.. toctree::
   :numbered:

   appetite.rst
   interpreter.rst
   introduction.rst
   controlflow.rst
   datastructures.rst
   modules.rst
   inputoutput.rst
   errors.rst
   classes.rst
   stdlib.rst
   stdlib2.rst
   venv.rst
   whatnow.rst
   interactive.rst
   floatingpoint.rst
   appendix.rst
```

## Symbols

```txt
 1| .. _tutorial-index:
 3| ######################
 4|   The Python Tutorial
 5| ######################
 7| .. Tip:: This tutorial is designed for
11| Python is an easy to learn, powerful programming language. It has efficient
12| high-level data structures and a simple but effective approach to
13| object-oriented programming. Python's elegant syntax and dynamic typing,
14| together with its interpreted nature, make it an ideal language for scripting
15| and rapid application development in many areas on most platforms.
17| The Python interpreter and the extensive standard library are freely available
18| in source or binary form for all major platforms from the Python website,
19| https://www.python.org/, and may be freely distributed. The same site also
20| contains distributions of and pointers to many free third party Python modules,
21| programs and tools, and additional documentation.
23| The Python interpreter is easily extended with new functions and data types
24| implemented in C or C++ (or other languages callable from C). Python is also
25| suitable as an extension language for customizable applications.
27| This tutorial introduces the reader informally to the basic concepts and
28| features of the Python language and system. Be aware that it expects you to
29| have a basic understanding of programming in general. It helps to have a Python
30| interpreter handy for hands-on experience, but all examples are self-contained,
31| so the tutorial can be read off-line as well.
33| For a description of standard objects and modules, see :ref:`library-index`.
34| :ref:`reference-index` gives a more formal definition of the language.  To write
35| extensions in C or C++, read :ref:`extending-index` and
36| :ref:`c-api-index`. There ar

... [truncated 107 chars] ...

over every single
39| feature, or even every commonly used feature. Instead, it introduces many of
40| Python's most noteworthy features, and will give you a good idea of the
41| language's flavor and style. After reading it, you will be able to read and
42| write Python modules and programs, and you will be ready to learn more about the
43| various Python library modules described in :ref:`library-index`.
45| The :ref:`glossary` is also worth going through.
47| .. toctree::
48|    :numbered:
50|    appetite.rst
51|    interpreter.rst
52|    introduction.rst
53|    controlflow.rst
54|    datastructures.rst
55|    modules.rst
56|    inputoutput.rst
57|    errors.rst
58|    classes.rst
59|    stdlib.rst
60|    stdlib2.rst
61|    venv.rst
62|    whatnow.rst
63|    interactive.rst
64|    floatingpoint.rst
65|    appendix.rst
```
