# Python (.py)

Source sample: `py/00-httpx-client.py`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 65713 | - | - |
| content-view | 51725 | 21.3% | 2.107 ms |
| applyMinification | 51801 | 21.2% | 2.08 ms |
| sync minify | 51801 | 21.2% | 2.037 ms |
| async minify | 51801 | 21.2% | 2.063 ms |
| symbols | 30668 | 53.3% | 5.68 ms |

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

... [truncated 50001 chars] ...

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

... [truncated 50001 chars] ...

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

... [truncated 50001 chars] ...

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
  15|     DEFAULT_LIMITS,
  16|     DEFAULT_MAX_REDIRECTS,
  17|     DEFAULT_TIMEOUT_CONFIG,
  18|     Limits,
  19|     Proxy,
  20|     Timeout,
  21| )
  22| from ._decoders import SUPPORTED_DECODERS
  23| from ._exceptions import (
  24|     InvalidURL,
  25|     RemoteProtocolError,
  26|     TooManyRedirects,
  27|     request_context,
  28| )
  29| from ._models import Cookies, Headers, Request, Response
  30| from ._status_codes import codes
  31| from ._transports.base import AsyncBaseTransport, BaseTransport
  32| from ._transports.default import AsyncHTTPTransport, HTTPTransport
  33| from ._types import (
  34|     AsyncByteStream,
  35|     AuthTypes,
  36|     CertTypes,
  37|     CookieTypes,
  38|     HeaderTypes,
  39|     ProxyTypes,
  40|     QueryParamTypes,
  41|     RequestContent,
  42|     RequestData,
  43|     RequestExtensions,
  44|     RequestFiles,
  45|     SyncByteStream,
  46|     TimeoutTypes,
  47| )
  48| from ._urls import URL, QueryParams
  49| from ._utils import URLPattern, get_environment_proxies
  51| if typing.TYPE_CHECKING:
  52|     import ssl  # pragma: no cover
  54| __all__ = ["USE_CLIENT_DEFAULT", "AsyncClient", "Client"]
  58| T = typing.TypeVar("T", bound="Client")
  59| U = typing.TypeVar("U", bound="AsyncClient")
  62| def _is_https_redirect(url: URL, location: URL) -> bool:
  77| def _port_or_default(

... [truncated 28068 chars] ...

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
