**Answer:** At 4b3949cd9e, `fastapi/security/` has seven modules. The security schemes all inherit from `SecurityBase`, which is defined in `base.py`. I read the class definitions through the GitHub API at that commit. I did not open `utils.py` beyond its filename, and the line numbers below are the `class` lines.

**Modules and the classes each defines**
- `__init__.py` has no classes of its own. It re-exports the public names with the `X as X` idiom (lines 1–15):
  - `APIKeyCookie`, `APIKeyHeader`, `APIKeyQuery`
  - `HTTPAuthorizationCredentials`, `HTTPBasic`, `HTTPBasicCredentials`, `HTTPBearer`, `HTTPDigest`
  - `OAuth2`, `OAuth2AuthorizationCodeBearer`, `OAuth2PasswordBearer`, `OAuth2PasswordRequestForm`, `OAuth2PasswordRequestFormStrict`, `SecurityScopes`
  - `OpenIdConnect`
  - It does not re-export `SecurityBase`, `APIKeyBase` or `HTTPBase`.
- `base.py` defines `SecurityBase` (line 4).
- `api_key.py` defines:
  - `APIKeyBase(SecurityBase)` (line 11)
  - `APIKeyQuery(APIKeyBase)` (line 55)
  - `APIKeyHeader(APIKeyBase)` (line 147)
  - `APIKeyCookie(APIKeyBase)` (line 235)
- `http.py` defines:
  - `HTTPBasicCredentials(BaseModel)` (line 16)
  - `HTTPAuthorizationCredentials(BaseModel)` (line 29)
  - `HTTPBase(SecurityBase)` (line 69)
  - `HTTPBasic(HTTPBase)` (line 105)
  - `HTTPBearer(HTTPBase)` (line 222)
  - `HTTPDigest(HTTPBase)` (line 319)
- `oauth2.py` defines:
  - `OAuth2PasswordRequestForm` (line 14), a plain class
  - `OAuth2PasswordRequestFormStrict(OAuth2PasswordRequestForm)` (line 162)
  - `OAuth2(SecurityBase)` (line 330)
  - `OAuth2PasswordBearer(OAuth2)` (line 433)
  - `OAuth2AuthorizationCodeBearer(OAuth2)` (line 547)
  - `SecurityScopes` (line 653), a plain class
- `open_id_connect_url.py` defines `OpenIdConnect(SecurityBase)` (line 11).
- `utils.py` holds helpers. `http.py` and `oauth2.py` import `get_authorization_scheme_param` from it. I did not list its contents.

**Common base class:** `fastapi.security.base.SecurityBase`. `APIKeyBase`, `HTTPBase`, `OAuth2` and `OpenIdConnect` each inherit from it directly. The concrete schemes inherit through those four.

The credential and form classes (`HTTPBasicCredentials`, `HTTPAuthorizationCredentials`, `OAuth2PasswordRequestForm` and its Strict variant, `SecurityScopes`) are not security schemes and do not inherit from `SecurityBase`.