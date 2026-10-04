`fastapi.security` is a package of seven modules. All the security schemes share the base class `SecurityBase`, which `base.py` defines. The module list and `__init__.py` come from commit 4b3949cd9e. The class listing comes from a code search that ran against a different commit, 5f9fc5c (the default branch at search time). I didn't re-read each module at 4b3949cd9e, so treat the per-module classes as very likely but not fully verified for that commit.

**Modules** (directory listing at 4b3949cd9e5932f76217131fceabc74145c3db24): `__init__.py`, `api_key.py`, `base.py`, `http.py`, `oauth2.py`, `open_id_connect_url.py`, `utils.py`.

**Classes per module:**
- `base.py` defines `SecurityBase`.
- `api_key.py` defines:
  - `APIKeyBase(SecurityBase)`
  - `APIKeyQuery(APIKeyBase)`
  - `APIKeyHeader(APIKeyBase)`
  - `APIKeyCookie(APIKeyBase)`
- `http.py` defines:
  - `HTTPBasicCredentials(BaseModel)`
  - `HTTPAuthorizationCredentials(BaseModel)`
  - `HTTPBase(SecurityBase)`
  - `HTTPBasic(HTTPBase)`
  - `HTTPBearer(HTTPBase)`
  - `HTTPDigest(HTTPBase)`
- `oauth2.py` defines:
  - `OAuth2PasswordRequestForm`, a plain dependency class
  - `OAuth2PasswordRequestFormStrict(OAuth2PasswordRequestForm)`
  - `OAuth2(SecurityBase)`
  - `OAuth2PasswordBearer(OAuth2)`
  - `OAuth2AuthorizationCodeBearer(OAuth2)`
  - `SecurityScopes`, a plain class
- `open_id_connect_url.py` defines `OpenIdConnect(SecurityBase)`.
- `utils.py` didn't appear in the class search, so it presumably holds only helper functions. I didn't read it.

**Re-exports:** `__init__.py` (lines 1–15 at 4b3949cd9e) re-exports these names with `X as X`:
- `APIKeyCookie`, `APIKeyHeader`, `APIKeyQuery`
- `HTTPAuthorizationCredentials`, `HTTPBasic`, `HTTPBasicCredentials`, `HTTPBearer`, `HTTPDigest`
- `OAuth2`, `OAuth2AuthorizationCodeBearer`, `OAuth2PasswordBearer`, `OAuth2PasswordRequestForm`, `OAuth2PasswordRequestFormStrict`, `SecurityScopes`
- `OpenIdConnect`

`SecurityBase`, `APIKeyBase` and `HTTPBase` are not re-exported.

**Inheritance:** every scheme derives from `SecurityBase`, either directly or through an intermediate base:
- directly: `OAuth2` and `OpenIdConnect`
- through `APIKeyBase`: the three `APIKey*` classes
- through `HTTPBase`: `HTTPBasic`, `HTTPBearer` and `HTTPDigest`
- through `OAuth2`: `OAuth2PasswordBearer` and `OAuth2AuthorizationCodeBearer`

`HTTPBasicCredentials` and `HTTPAuthorizationCredentials` are pydantic models. `OAuth2PasswordRequestForm`, its Strict subclass and `SecurityScopes` are plain classes. None of these five is a security scheme.