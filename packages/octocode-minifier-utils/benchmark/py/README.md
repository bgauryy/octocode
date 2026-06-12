# Python (.py)

Source sample: `py/00-httpx-client.py`

Strategy: `conservative`

Agent rating: **8.2/10 (strong)**

Agent understanding from minified output: **9.8/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 65713 | - | - | - |
| content-view | 51725 | 21.3% | 20.669 ms | 7.8/10 |
| applyMinification | 51725 | 21.3% | 29.617 ms | 7.8/10 |
| sync minify | 51725 | 21.3% | 35.083 ms | 7.8/10 |
| async minify | 51725 | 21.3% | 20.236 ms | 7.8/10 |
| symbols | 23947 | 63.6% | 1.415 ms | 9/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 8/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 65713 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 51725 | 21.3% | 9.8/10 excellent | 10/10 | 10/10 |
| minify | 51725 | 21.3% | 9.8/10 excellent | 10/10 | 10/10 |
| symbols | 23947 | 63.6% | 8.7/10 strong | 6.7/10 | 9.9/10 |

## Notes

- conservative text strategy.

## Before Excerpt

```py
from __future__ import annotations

import datetime
import enum
import logging
import time
import typing
import warnings
from contextlib import asynccontextmanager, contextmanager
from types import TracebackType

from .__version__ import __version__
from ._auth import Auth, BasicAuth, FunctionAuth
from ._config import (
    DEFAULT_LIMITS,
    DEFAULT_MAX_REDIRECTS,
    DEFAULT_TIMEOUT_CONFIG,
    Limits,
    Proxy,
    Timeout,
)
from ._decoders import SUPPORTED_DECODERS
from ._exceptions import (
    InvalidURL,
    RemoteProtocolError,
    TooManyRedirects,
    request_context,
)
from ._models import Cookies, Headers, Request, Response
from ._status_codes import codes
from ._transports.base import AsyncBaseTransport, BaseTransport
from ._transports.default import AsyncHTTPTransport, HTTPTransport
from ._types import (
    AsyncByteStream,
    AuthTypes,
    CertTypes,
    CookieTypes,
    HeaderTypes,
    ProxyTypes,
    QueryParamTypes,
    RequestContent,
    RequestData,
    RequestExtensions,
    RequestFiles,
    SyncByteStream,
    TimeoutTypes,
)
from ._urls import URL, QueryParams
from ._utils import URLPattern, get_environment_proxies

if typing.TYPE_CHECKING:
    import ssl  # pragma: no cove

... [truncated 63913 chars] ...

unts.values():
            if proxy is not None:
                await proxy.__aenter__()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None = None,
        exc_value: BaseException | None = None,
        traceback: TracebackType | None = None,
    ) -> None:
        self._state = ClientState.CLOSED

        await self._transport.__aexit__(exc_type, exc_value, traceback)
        for proxy in self._mounts.values():
            if proxy is not None:
                await proxy.__aexit__(exc_type, exc_value, traceback)

```

## Content-View Excerpt

```py
from __future__ import annotations

import datetime
import enum
import logging
import time
import typing
import warnings
from contextlib import asynccontextmanager, contextmanager
from types import TracebackType

from .__version__ import __version__
from ._auth import Auth, BasicAuth, FunctionAuth
from ._config import (
    DEFAULT_LIMITS,
    DEFAULT_MAX_REDIRECTS,
    DEFAULT_TIMEOUT_CONFIG,
    Limits,
    Proxy,
    Timeout,
)
from ._decoders import SUPPORTED_DECODERS
from ._exceptions import (
    InvalidURL,
    RemoteProtocolError,
    TooManyRedirects,
    request_context,
)
from ._models import Cookies, Headers, Request, Response
from ._status_codes import codes
from ._transports.base import AsyncBaseTransport, BaseTransport
from ._transports.default import AsyncHTTPTransport, HTTPTransport
from ._types import (
    AsyncByteStream,
    AuthTypes,
    CertTypes,
    CookieTypes,
    HeaderTypes,
    ProxyTypes,
    QueryParamTypes,
    RequestContent,
    RequestData,
    RequestExtensions,
    RequestFiles,
    SyncByteStream,
    TimeoutTypes,
)
from ._urls import URL, QueryParams
from ._utils import URLPattern, get_environment_proxies

if typing.TYPE_CHECKING:
    import ssl

__all__ = ["USE_C

... [truncated 49925 chars] ...

ounts.values():
            if proxy is not None:
                await proxy.__aenter__()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None = None,
        exc_value: BaseException | None = None,
        traceback: TracebackType | None = None,
    ) -> None:
        self._state = ClientState.CLOSED

        await self._transport.__aexit__(exc_type, exc_value, traceback)
        for proxy in self._mounts.values():
            if proxy is not None:
                await proxy.__aexit__(exc_type, exc_value, traceback)
```

## Apply Minification Excerpt

```py
from __future__ import annotations

import datetime
import enum
import logging
import time
import typing
import warnings
from contextlib import asynccontextmanager, contextmanager
from types import TracebackType

from .__version__ import __version__
from ._auth import Auth, BasicAuth, FunctionAuth
from ._config import (
    DEFAULT_LIMITS,
    DEFAULT_MAX_REDIRECTS,
    DEFAULT_TIMEOUT_CONFIG,
    Limits,
    Proxy,
    Timeout,
)
from ._decoders import SUPPORTED_DECODERS
from ._exceptions import (
    InvalidURL,
    RemoteProtocolError,
    TooManyRedirects,
    request_context,
)
from ._models import Cookies, Headers, Request, Response
from ._status_codes import codes
from ._transports.base import AsyncBaseTransport, BaseTransport
from ._transports.default import AsyncHTTPTransport, HTTPTransport
from ._types import (
    AsyncByteStream,
    AuthTypes,
    CertTypes,
    CookieTypes,
    HeaderTypes,
    ProxyTypes,
    QueryParamTypes,
    RequestContent,
    RequestData,
    RequestExtensions,
    RequestFiles,
    SyncByteStream,
    TimeoutTypes,
)
from ._urls import URL, QueryParams
from ._utils import URLPattern, get_environment_proxies

if typing.TYPE_CHECKING:
    import ssl

__all__ = ["USE_C

... [truncated 49925 chars] ...

ounts.values():
            if proxy is not None:
                await proxy.__aenter__()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None = None,
        exc_value: BaseException | None = None,
        traceback: TracebackType | None = None,
    ) -> None:
        self._state = ClientState.CLOSED

        await self._transport.__aexit__(exc_type, exc_value, traceback)
        for proxy in self._mounts.values():
            if proxy is not None:
                await proxy.__aexit__(exc_type, exc_value, traceback)
```

## Sync Minify Excerpt

```py
from __future__ import annotations

import datetime
import enum
import logging
import time
import typing
import warnings
from contextlib import asynccontextmanager, contextmanager
from types import TracebackType

from .__version__ import __version__
from ._auth import Auth, BasicAuth, FunctionAuth
from ._config import (
    DEFAULT_LIMITS,
    DEFAULT_MAX_REDIRECTS,
    DEFAULT_TIMEOUT_CONFIG,
    Limits,
    Proxy,
    Timeout,
)
from ._decoders import SUPPORTED_DECODERS
from ._exceptions import (
    InvalidURL,
    RemoteProtocolError,
    TooManyRedirects,
    request_context,
)
from ._models import Cookies, Headers, Request, Response
from ._status_codes import codes
from ._transports.base import AsyncBaseTransport, BaseTransport
from ._transports.default import AsyncHTTPTransport, HTTPTransport
from ._types import (
    AsyncByteStream,
    AuthTypes,
    CertTypes,
    CookieTypes,
    HeaderTypes,
    ProxyTypes,
    QueryParamTypes,
    RequestContent,
    RequestData,
    RequestExtensions,
    RequestFiles,
    SyncByteStream,
    TimeoutTypes,
)
from ._urls import URL, QueryParams
from ._utils import URLPattern, get_environment_proxies

if typing.TYPE_CHECKING:
    import ssl

__all__ = ["USE_C

... [truncated 49925 chars] ...

ounts.values():
            if proxy is not None:
                await proxy.__aenter__()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None = None,
        exc_value: BaseException | None = None,
        traceback: TracebackType | None = None,
    ) -> None:
        self._state = ClientState.CLOSED

        await self._transport.__aexit__(exc_type, exc_value, traceback)
        for proxy in self._mounts.values():
            if proxy is not None:
                await proxy.__aexit__(exc_type, exc_value, traceback)
```

## Async Minify Excerpt

```py
from __future__ import annotations

import datetime
import enum
import logging
import time
import typing
import warnings
from contextlib import asynccontextmanager, contextmanager
from types import TracebackType

from .__version__ import __version__
from ._auth import Auth, BasicAuth, FunctionAuth
from ._config import (
    DEFAULT_LIMITS,
    DEFAULT_MAX_REDIRECTS,
    DEFAULT_TIMEOUT_CONFIG,
    Limits,
    Proxy,
    Timeout,
)
from ._decoders import SUPPORTED_DECODERS
from ._exceptions import (
    InvalidURL,
    RemoteProtocolError,
    TooManyRedirects,
    request_context,
)
from ._models import Cookies, Headers, Request, Response
from ._status_codes import codes
from ._transports.base import AsyncBaseTransport, BaseTransport
from ._transports.default import AsyncHTTPTransport, HTTPTransport
from ._types import (
    AsyncByteStream,
    AuthTypes,
    CertTypes,
    CookieTypes,
    HeaderTypes,
    ProxyTypes,
    QueryParamTypes,
    RequestContent,
    RequestData,
    RequestExtensions,
    RequestFiles,
    SyncByteStream,
    TimeoutTypes,
)
from ._urls import URL, QueryParams
from ._utils import URLPattern, get_environment_proxies

if typing.TYPE_CHECKING:
    import ssl

__all__ = ["USE_C

... [truncated 49925 chars] ...

ounts.values():
            if proxy is not None:
                await proxy.__aenter__()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None = None,
        exc_value: BaseException | None = None,
        traceback: TracebackType | None = None,
    ) -> None:
        self._state = ClientState.CLOSED

        await self._transport.__aexit__(exc_type, exc_value, traceback)
        for proxy in self._mounts.values():
            if proxy is not None:
                await proxy.__aexit__(exc_type, exc_value, traceback)
```

## Symbols

```txt
   1| from __future__ import annotations
   3| import datetime
   4| import enum
   5| import logging
   6| import time
   7| import typing
   8| import warnings
   9| from contextlib import asynccontextmanager, contextmanager
  10| from types import TracebackType
  12| from .__version__ import __version__
  13| from ._auth import Auth, BasicAuth, FunctionAuth
  14| from ._config import (
  22| from ._decoders import SUPPORTED_DECODERS
  23| from ._exceptions import (
  29| from ._models import Cookies, Headers, Request, Response
  30| from ._status_codes import codes
  31| from ._transports.base import AsyncBaseTransport, BaseTransport
  32| from ._transports.default import AsyncHTTPTransport, HTTPTransport
  33| from ._types import (
  48| from ._urls import URL, QueryParams
  49| from ._utils import URLPattern, get_environment_proxies
  52|     import ssl  # pragma: no cover
  54| __all__ = ["USE_CLIENT_DEFAULT", "AsyncClient", "Client"]
  62| def _is_https_redirect(url: URL, location: URL) -> bool:
  77| def _port_or_default(url: URL) -> int | None:
  83| def _same_origin(url: URL, other: URL) -> bool:
  94| class UseClientDefault:
 125| class ClientState(enum.Enum):
 139| class BoundSyncStream(SyncByteStream):
 145|     def __init__(
 146|         self, stream: SyncByteStream, response: Response, start: float
 147|     ) -> None:
 152|     def __iter__(self) -> typing.Iterator[bytes]:
 156|     def close(self) -> None:
 162| class BoundAsyncStream(AsyncByteStream):
 168|     def __init__(
 169|         self, stream: AsyncByteStream, response: Response, start: float
 170|     ) -> None:
 175|     async def __aiter__(self) -> typing.AsyncIterator[bytes]:
 179|     async def aclose(self) -> None:
 188| class BaseClient:
 189|     def __

... [truncated 21347 chars] ...

|         url: URL | str,
1952|         *,
1953|         params: QueryParamTypes | None = None,
1954|         headers: HeaderTypes | None = None,
1955|         cookies: CookieTypes | None = None,
1956|         auth: AuthTypes | UseClientDefault = USE_CLIENT_DEFAULT,
1957|         follow_redirects: bool | UseClientDefault = USE_CLIENT_DEFAULT,
1958|         timeout: TimeoutTypes | UseClientDefault = USE_CLIENT_DEFAULT,
1959|         extensions: RequestExtensions | None = None,
1960|     ) -> Response:
1978|     async def aclose(self) -> None:
1990|     async def __aenter__(self: U) -> U:
2008|     async def __aexit__(
2009|         self,
2010|         exc_type: type[BaseException] | None = None,
2011|         exc_value: BaseException | None = None,
2012|         traceback: TracebackType | None = None,
2013|     ) -> None:
```
