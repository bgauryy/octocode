`fastapi.security` is a package of six modules. Every security scheme class inherits from `SecurityBase`, which `base.py` defines. All line numbers below are at commit 4b3949cd9e.

**Modules and the public classes each defines**
- `__init__.py` defines no classes. It re-exports names from the other modules with the `X as X` pattern (`__init__.py:1-15`).
- `base.py` defines `SecurityBase` (`base.py:4`).
- `api_key.py` defines:
  - `APIKeyBase(SecurityBase)` (line 11), an intermediate base class that `__init__.py` does not export.
  - `APIKeyQuery` (line 55).
  - `APIKeyHeader` (line 147).
  - `APIKeyCookie` (line 235).
  - All three concrete classes subclass `APIKeyBase`.
- `http.py` defines:
  - `HTTPBasicCredentials(BaseModel)` (line 16), a pydantic model rather than a scheme.
  - `HTTPAuthorizationCredentials(BaseModel)` (line 29), also a pydantic model.
  - `HTTPBase(SecurityBase)` (line 69), which `__init__.py` does not export.
  - `HTTPBasic` (line 105), `HTTPBearer` (line 222) and `HTTPDigest` (line 319), all subclassing `HTTPBase`.
- `oauth2.py` defines:
  - `OAuth2PasswordRequestForm` (line 14), a plain class that is a form dependency rather than a scheme.
  - `OAuth2PasswordRequestFormStrict(OAuth2PasswordRequestForm)` (line 162).
  - `OAuth2(SecurityBase)` (line 330).
  - `OAuth2PasswordBearer(OAuth2)` (line 433).
  - `OAuth2AuthorizationCodeBearer(OAuth2)` (line 547).
  - `SecurityScopes` (line 653), a plain helper class.
- `open_id_connect_url.py` defines `OpenIdConnect(SecurityBase)` (line 11).
- `utils.py` is in the directory listing, but I did not open it. I assume it holds helper functions, not classes.

**Common base class**
- `SecurityBase` is imported from `fastapi.security.base` by `api_key.py:5`, `http.py:9`, `oauth2.py:8` and `open_id_connect_url.py:5`.
- Every scheme class reaches it directly or through `APIKeyBase`, `HTTPBase` or `OAuth2`.
- `base.py` also imports the OpenAPI model `SecurityBase` from `fastapi.openapi.models` under the alias `SecurityBaseModel` (`base.py:1`). I did not read the body of the `SecurityBase` class.

**Uncertainty**
- My grep matched only top-level `class` lines, so any nested or conditionally defined classes would not appear.