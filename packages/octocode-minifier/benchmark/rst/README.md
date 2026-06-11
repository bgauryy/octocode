# reStructuredText (.rst)

Source sample: `rst/cpython-tutorial-index.rst`

Strategy: `conservative`

Agent rating: **6.3/10 (fair)**

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
| content-view | 2569 | 1.8% | 0.094 ms | 6.3/10 |
| applyMinification | 2569 | 1.8% | 0.084 ms | 6.3/10 |
| sync minify | 2569 | 1.8% | 0.081 ms | 6.3/10 |
| async minify | 2569 | 1.8% | 0.084 ms | 6.3/10 |
| symbols | n/a | n/a | 0.001 ms | n/a |

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

## Symbols

```txt
No symbols returned for this sample.
```
