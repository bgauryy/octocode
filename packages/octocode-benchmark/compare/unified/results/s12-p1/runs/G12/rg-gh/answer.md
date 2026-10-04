**Answer:** At 4b3949cd9e, `fastapi/security/` has seven modules. Every security scheme inherits from `SecurityBase`, which is defined in `base.py`. I read the class and import lines of each module through the GitHub API. I did not open `utils.py`.

**Modules and the classes each defines** (all paths under `fastapi/security/`):
- `__init__.py` defines no classes. It re-exports the public API with `from .x import Y as Y`, at lines 1–15. The re-exports are:
  - `APIKeyCookie`, `APIKeyHeader`, `APIKeyQuery`
  - `HTTPAuthorizationCredentials`, `HTTPBasic`, `HTTPBasicCredentials`, `HTTPBearer`, `HTTPDigest`
  - `OAuth2`, `OAuth2AuthorizationCodeBearer`, `OAuth2PasswordBearer`, `OAuth2PasswordRequestForm`, `OAuth2PasswordRequestFormStrict`, `SecurityScopes`
  - `OpenIdConnect`
- `base.py` defines `SecurityBase` (line 4).
- `api_key.py` defines:
  - `APIKeyBase(SecurityBase)` (line 11)
  - `APIKeyQuery`, `APIKeyHeader` and `APIKeyCookie`, each subclassing `APIKeyBase` (lines 55, 147, 235)
- `http.py` defines:
  - `HTTPBasicCredentials(BaseModel)` (line 16)
  - `HTTPAuthorizationCredentials(BaseModel)` (line 29)
  - `HTTPBase(SecurityBase)` (line 69)
  - `HTTPBasic`, `HTTPBearer` and `HTTPDigest`, each subclassing `HTTPBase` (lines 105, 222, 319)
- `oauth2.py` defines:
  - `OAuth2PasswordRequestForm` (line 14) and `OAuth2PasswordRequestFormStrict`, which subclasses it (line 162). Neither is a security scheme.
  - `OAuth2(SecurityBase)` (line 330)
  - `OAuth2PasswordBearer(OAuth2)` (line 433)
  - `OAuth2AuthorizationCodeBearer(OAuth2)` (line 547)
  - `SecurityScopes` (line 653), a plain class
- `open_id_connect_url.py` defines `OpenIdConnect(SecurityBase)` (line 11).
- `utils.py` exists, and `http.py` and `oauth2.py` import `get_authorization_scheme_param` from it. I did not list its contents. It is not re-exported by `__init__.py`.

**Common base class:** `fastapi.security.base.SecurityBase` is the base of all the schemes.
- It is the base of `APIKeyBase`, `HTTPBase`, `OAuth2` and `OpenIdConnect`.
- The concrete schemes inherit from it indirectly:
  - the API key classes through `APIKeyBase`
  - `HTTPBasic`, `HTTPBearer` and `HTTPDigest` through `HTTPBase`
  - the OAuth2 bearer classes through `OAuth2`
- The credential and form classes (`HTTPBasicCredentials`, `HTTPAuthorizationCredentials`, `OAuth2PasswordRequestForm` and its Strict variant) and `SecurityScopes` are not schemes and do not inherit from it.

**Uncertainty:** I did not read the body of `SecurityBase`. `base.py` line 1 imports `SecurityBase as SecurityBaseModel` from `fastapi.openapi.models`, so it presumably holds that OpenAPI model. I did not confirm this.