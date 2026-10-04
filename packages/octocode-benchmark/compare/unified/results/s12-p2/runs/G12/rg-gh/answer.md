**Answer:** At 4b3949cd9e, `fastapi/security/` has six modules plus `__init__.py`. Every security scheme inherits from `SecurityBase`, which `base.py` defines at line 4. I listed the directory and read each module's `class` lines through the GitHub API. Line numbers below are the `class` lines at that commit.

**Modules and the classes each defines**
- `base.py`
  - `SecurityBase` (line 4).
- `api_key.py`
  - `APIKeyBase(SecurityBase)` (line 11). This is an internal helper and isn't re-exported.
  - `APIKeyQuery(APIKeyBase)` (line 55).
  - `APIKeyHeader(APIKeyBase)` (line 147).
  - `APIKeyCookie(APIKeyBase)` (line 235).
- `http.py`
  - `HTTPBasicCredentials(BaseModel)` (line 16). This is a pydantic model, not a scheme.
  - `HTTPAuthorizationCredentials(BaseModel)` (line 29). This is also a pydantic model.
  - `HTTPBase(SecurityBase)` (line 69). This is not re-exported.
  - `HTTPBasic(HTTPBase)` (line 105).
  - `HTTPBearer(HTTPBase)` (line 222).
  - `HTTPDigest(HTTPBase)` (line 319).
- `oauth2.py`
  - `OAuth2PasswordRequestForm` (line 14). This is a plain form-dependency class.
  - `OAuth2PasswordRequestFormStrict(OAuth2PasswordRequestForm)` (line 162).
  - `OAuth2(SecurityBase)` (line 330).
  - `OAuth2PasswordBearer(OAuth2)` (line 433).
  - `OAuth2AuthorizationCodeBearer(OAuth2)` (line 547).
  - `SecurityScopes` (line 653). This is a helper, not a scheme.
- `open_id_connect_url.py`
  - `OpenIdConnect(SecurityBase)` (line 11).
- `utils.py`
  - I only listed this file and didn't read it. `http.py` and `oauth2.py` import `get_authorization_scheme_param` from it.
- `__init__.py` (lines 1–15)
  - It re-exports these 15 names using the `X as X` form:
    - `APIKeyCookie`, `APIKeyHeader`, `APIKeyQuery`
    - `HTTPAuthorizationCredentials`, `HTTPBasic`, `HTTPBasicCredentials`, `HTTPBearer`, `HTTPDigest`
    - `OAuth2`, `OAuth2AuthorizationCodeBearer`, `OAuth2PasswordBearer`, `OAuth2PasswordRequestForm`, `OAuth2PasswordRequestFormStrict`, `SecurityScopes`
    - `OpenIdConnect`
  - It does not export `SecurityBase`, `APIKeyBase` or `HTTPBase`.

**Common base class**
- The shared base is `fastapi.security.base.SecurityBase`.
- The API-key, HTTP, OAuth2 and OpenID Connect schemes all inherit from it, directly or through `APIKeyBase`, `HTTPBase` or `OAuth2`.
- `api_key.py`, `http.py`, `oauth2.py` and `open_id_connect_url.py` each import it with `from fastapi.security.base import SecurityBase`.
- `base.py` line 1 imports the OpenAPI model as `SecurityBaseModel`, which is a different class.

**Uncertainty:** I didn't read the body of `SecurityBase` or `utils.py`, so I haven't checked what attributes the base class declares or which functions `utils.py` contains beyond the one imported name.