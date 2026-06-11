# Python (.py)

Source sample: `py/sessions.py`

Strategy: `conservative`

Agent rating: **8.6/10 (strong)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 34072 | - | - | - |
| content-view | 26625 | 21.9% | 3.998 ms | 7.8/10 |
| applyMinification | 26625 | 21.9% | 3.712 ms | 7.8/10 |
| sync minify | 26625 | 21.9% | 4.445 ms | 7.8/10 |
| async minify | 26625 | 21.9% | 3.439 ms | 7.8/10 |
| symbols | 4995 | 85.3% | 0.39 ms | 10/10 |

## Notes

- conservative text strategy.

## Before Excerpt

```py
"""
requests.sessions
~~~~~~~~~~~~~~~~~

This module provides a Session object to manage and persist settings across
requests (cookies, auth, proxies).
"""

from __future__ import annotations

import os
import sys
import time
from collections import OrderedDict
from collections.abc import Generator, Mapping, MutableMapping
from datetime import timedelta
from typing import TYPE_CHECKING, Any, cast

from ._internal_utils import to_native_string
from ._types import is_prepared as _is_prepared
from .adapters import HTTPAdapter
from .auth import _basic_auth_str
from .compat import cookielib, urljoin, urlparse
from .cookies import (
    RequestsCookieJar,
    cookiejar_from_dict,
    extract_cookies_to_jar,
    merge_cookies,
)
from .exceptions import (
    ChunkedEncodingError,
    ContentDecodingError,
    InvalidSchema,
    TooManyRedirects,
)
from .hooks import default_hooks, dispatch_hook

# formerly defined here, reexposed here for backward compatibility
from .models import (  # noqa: F401
    DEFAULT_REDIRECT_LIMIT,
    REDIRECT_STATI,
    PreparedRequest,
    Request,
    Response,
)
from .status_codes import codes
from .structures import CaseInsensitiveDict
from .utils import (  # noqa: F401
    DEFAUL

... [truncated 32272 chars] ...

ttrs__}
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        for attr, value in state.items():
            setattr(self, attr, value)


def session() -> Session:
    """
    Returns a :class:`Session` for context-management.

    .. deprecated:: 1.0.0

        This method has been deprecated since version 1.0.0 and is only kept for
        backwards compatibility. New code should use :class:`~requests.sessions.Session`
        to create a session. This may be removed at a future date.

    :rtype: Session
    """
    return Session()

```

## Content-View Excerpt

```py
"""
requests.sessions
~~~~~~~~~~~~~~~~~

This module provides a Session object to manage and persist settings across
requests (cookies, auth, proxies).
"""

from __future__ import annotations

import os
import sys
import time
from collections import OrderedDict
from collections.abc import Generator, Mapping, MutableMapping
from datetime import timedelta
from typing import TYPE_CHECKING, Any, cast

from ._internal_utils import to_native_string
from ._types import is_prepared as _is_prepared
from .adapters import HTTPAdapter
from .auth import _basic_auth_str
from .compat import cookielib, urljoin, urlparse
from .cookies import (
    RequestsCookieJar,
    cookiejar_from_dict,
    extract_cookies_to_jar,
    merge_cookies,
)
from .exceptions import (
    ChunkedEncodingError,
    ContentDecodingError,
    InvalidSchema,
    TooManyRedirects,
)
from .hooks import default_hooks, dispatch_hook

from .models import (
    DEFAULT_REDIRECT_LIMIT,
    REDIRECT_STATI,
    PreparedRequest,
    Request,
    Response,
)
from .status_codes import codes
from .structures import CaseInsensitiveDict
from .utils import (
    DEFAULT_PORTS,
    default_headers,
    get_auth_from_url,
    get_environ_proxies,
    get_netrc_aut

... [truncated 24825 chars] ...

_attrs__}
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        for attr, value in state.items():
            setattr(self, attr, value)

def session() -> Session:
    """
    Returns a :class:`Session` for context-management.

    .. deprecated:: 1.0.0

        This method has been deprecated since version 1.0.0 and is only kept for
        backwards compatibility. New code should use :class:`~requests.sessions.Session`
        to create a session. This may be removed at a future date.

    :rtype: Session
    """
    return Session()
```

## Apply Minification Excerpt

```py
"""
requests.sessions
~~~~~~~~~~~~~~~~~

This module provides a Session object to manage and persist settings across
requests (cookies, auth, proxies).
"""

from __future__ import annotations

import os
import sys
import time
from collections import OrderedDict
from collections.abc import Generator, Mapping, MutableMapping
from datetime import timedelta
from typing import TYPE_CHECKING, Any, cast

from ._internal_utils import to_native_string
from ._types import is_prepared as _is_prepared
from .adapters import HTTPAdapter
from .auth import _basic_auth_str
from .compat import cookielib, urljoin, urlparse
from .cookies import (
    RequestsCookieJar,
    cookiejar_from_dict,
    extract_cookies_to_jar,
    merge_cookies,
)
from .exceptions import (
    ChunkedEncodingError,
    ContentDecodingError,
    InvalidSchema,
    TooManyRedirects,
)
from .hooks import default_hooks, dispatch_hook

from .models import (
    DEFAULT_REDIRECT_LIMIT,
    REDIRECT_STATI,
    PreparedRequest,
    Request,
    Response,
)
from .status_codes import codes
from .structures import CaseInsensitiveDict
from .utils import (
    DEFAULT_PORTS,
    default_headers,
    get_auth_from_url,
    get_environ_proxies,
    get_netrc_aut

... [truncated 24825 chars] ...

_attrs__}
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        for attr, value in state.items():
            setattr(self, attr, value)

def session() -> Session:
    """
    Returns a :class:`Session` for context-management.

    .. deprecated:: 1.0.0

        This method has been deprecated since version 1.0.0 and is only kept for
        backwards compatibility. New code should use :class:`~requests.sessions.Session`
        to create a session. This may be removed at a future date.

    :rtype: Session
    """
    return Session()
```

## Sync Minify Excerpt

```py
"""
requests.sessions
~~~~~~~~~~~~~~~~~

This module provides a Session object to manage and persist settings across
requests (cookies, auth, proxies).
"""

from __future__ import annotations

import os
import sys
import time
from collections import OrderedDict
from collections.abc import Generator, Mapping, MutableMapping
from datetime import timedelta
from typing import TYPE_CHECKING, Any, cast

from ._internal_utils import to_native_string
from ._types import is_prepared as _is_prepared
from .adapters import HTTPAdapter
from .auth import _basic_auth_str
from .compat import cookielib, urljoin, urlparse
from .cookies import (
    RequestsCookieJar,
    cookiejar_from_dict,
    extract_cookies_to_jar,
    merge_cookies,
)
from .exceptions import (
    ChunkedEncodingError,
    ContentDecodingError,
    InvalidSchema,
    TooManyRedirects,
)
from .hooks import default_hooks, dispatch_hook

from .models import (
    DEFAULT_REDIRECT_LIMIT,
    REDIRECT_STATI,
    PreparedRequest,
    Request,
    Response,
)
from .status_codes import codes
from .structures import CaseInsensitiveDict
from .utils import (
    DEFAULT_PORTS,
    default_headers,
    get_auth_from_url,
    get_environ_proxies,
    get_netrc_aut

... [truncated 24825 chars] ...

_attrs__}
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        for attr, value in state.items():
            setattr(self, attr, value)

def session() -> Session:
    """
    Returns a :class:`Session` for context-management.

    .. deprecated:: 1.0.0

        This method has been deprecated since version 1.0.0 and is only kept for
        backwards compatibility. New code should use :class:`~requests.sessions.Session`
        to create a session. This may be removed at a future date.

    :rtype: Session
    """
    return Session()
```

## Async Minify Excerpt

```py
"""
requests.sessions
~~~~~~~~~~~~~~~~~

This module provides a Session object to manage and persist settings across
requests (cookies, auth, proxies).
"""

from __future__ import annotations

import os
import sys
import time
from collections import OrderedDict
from collections.abc import Generator, Mapping, MutableMapping
from datetime import timedelta
from typing import TYPE_CHECKING, Any, cast

from ._internal_utils import to_native_string
from ._types import is_prepared as _is_prepared
from .adapters import HTTPAdapter
from .auth import _basic_auth_str
from .compat import cookielib, urljoin, urlparse
from .cookies import (
    RequestsCookieJar,
    cookiejar_from_dict,
    extract_cookies_to_jar,
    merge_cookies,
)
from .exceptions import (
    ChunkedEncodingError,
    ContentDecodingError,
    InvalidSchema,
    TooManyRedirects,
)
from .hooks import default_hooks, dispatch_hook

from .models import (
    DEFAULT_REDIRECT_LIMIT,
    REDIRECT_STATI,
    PreparedRequest,
    Request,
    Response,
)
from .status_codes import codes
from .structures import CaseInsensitiveDict
from .utils import (
    DEFAULT_PORTS,
    default_headers,
    get_auth_from_url,
    get_environ_proxies,
    get_netrc_aut

... [truncated 24825 chars] ...

_attrs__}
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        for attr, value in state.items():
            setattr(self, attr, value)

def session() -> Session:
    """
    Returns a :class:`Session` for context-management.

    .. deprecated:: 1.0.0

        This method has been deprecated since version 1.0.0 and is only kept for
        backwards compatibility. New code should use :class:`~requests.sessions.Session`
        to create a session. This may be removed at a future date.

    :rtype: Session
    """
    return Session()
```

## Symbols

```txt
  9| from __future__ import annotations
 11| import os
 12| import sys
 13| import time
 14| from collections import OrderedDict
 15| from collections.abc import Generator, Mapping, MutableMapping
 16| from datetime import timedelta
 17| from typing import TYPE_CHECKING, Any, cast
 19| from ._internal_utils import to_native_string
 20| from ._types import is_prepared as _is_prepared
 21| from .adapters import HTTPAdapter
 22| from .auth import _basic_auth_str
 23| from .compat import cookielib, urljoin, urlparse
 24| from .cookies import (
 30| from .exceptions import (
 36| from .hooks import default_hooks, dispatch_hook
 39| from .models import (  # noqa: F401
 46| from .status_codes import codes
 47| from .structures import CaseInsensitiveDict
 48| from .utils import (  # noqa: F401
 62|     from http.cookiejar import CookieJar
 64|     from typing_extensions import Self, Unpack
 66|     from . import _types as _t
 67|     from .adapters import BaseAdapter
 76| def merge_setting(
 77|     request_setting: Any, session_setting: Any, dict_class: type = OrderedDict
 78| ) -> Any:
108| def merge_hooks(
109|     request_hooks: _t.HooksType,
110|     session_hooks: _t.HooksType,
111|     dict_class: type = OrderedDict,
112| ) -> _t.HooksType:
127| class SessionRedirectMixin:
132|     def send(self, request: PreparedRequest, **kwargs: Any) -> Response: ...
134|     def get_redirect_target(self, resp: Response) -> str | None:
154|     def should_strip_auth(self, old_url: str, new_url: str) -> bool:
186|     def resolve_redirects(
187|         self,
188|         resp: Response,
189|         req: PreparedRequest,
190|         stream: bool = False,
191|         timeout: _t.TimeoutType = None,
192|         verify: _t.VerifyType = True,
193|       

... [truncated 2395 chars] ...

riType, data: _t.DataType = None, **kwargs: Unpack[_t.DataKwargs]
730|     ) -> Response:
742|     def delete(self, url: _t.UriType, **kwargs: Unpack[_t.RequestKwargs]) -> Response:
752|     def send(self, request: PreparedRequest, **kwargs: Any) -> Response:
831|     def merge_environment_settings(
832|         self,
833|         url: str,
834|         proxies: dict[str, str] | None,
835|         stream: bool | None,
836|         verify: _t.VerifyType | None,
837|         cert: _t.CertType,
838|     ) -> dict[str, Any]:
870|     def get_adapter(self, url: str) -> BaseAdapter:
883|     def close(self) -> None:
888|     def mount(self, prefix: str, adapter: BaseAdapter) -> None:
899|     def __getstate__(self) -> dict[str, Any]:
903|     def __setstate__(self, state: dict[str, Any]) -> None:
908| def session() -> Session:
```
