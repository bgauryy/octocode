The security schemes all share the base class `SecurityBase`, defined in `fastapi/security/base.py`. The package is seven modules. One caveat: the module list and `__init__.py` come from commit 4b3949cd9e, but the class list comes from a code search that ran on a newer default-branch commit (5f9fc5c). I did not re-read each module at 4b3949cd9e, so the class names and line numbers below may differ slightly from that commit.

**Modules in `fastapi/security/`** (listed at 4b3949cd9e):
- `__init__.py`
- `api_key.py`
- `base.py`
- `http.py`
- `oauth2.py`
- `open_id_connect_url.py`
- `utils.py`

**Classes per module** (line numbers are from the 5f9fc5c search):
- **`base.py`**: `SecurityBase` (line 4).
- **`api_key.py`**:
  - `APIKeyBase(SecurityBase)` (line 11)
  - `APIKeyQuery(APIKeyBase)` (line 55)
  - `APIKeyHeader(APIKeyBase)` (line 147)
  - `APIKeyCookie(APIKeyBase)` (line 235)
- **`http.py`**:
  - `HTTPBasicCredentials(BaseModel)` (line 16)
  - `HTTPAuthorizationCredentials(BaseModel)` (line 29)
  - `HTTPBase(SecurityBase)` (line 69)
  - `HTTPBasic(HTTPBase)` (line 105)
  - `HTTPBearer(HTTPBase)` (line 222)
  - `HTTPDigest(HTTPBase)` (line 319)
- **`oauth2.py`**:
  - `OAuth2PasswordRequestForm` (line 14, a plain class used as a dependency)
  - `OAuth2PasswordRequestFormStrict(OAuth2PasswordRequestForm)` (line 162)
  - `OAuth2(SecurityBase)` (line 330)
  - `OAuth2PasswordBearer(OAuth2)` (line 433)
  - `OAuth2AuthorizationCodeBearer(OAuth2)` (line 547)
  - `SecurityScopes` (line 653, a plain class)
- **`open_id_connect_url.py`**: `OpenIdConnect(SecurityBase)` (line 11).
- **`utils.py`**: no classes turned up in the search. I did not read the file, so I assume it only holds helpers.

**Re-exports:** `fastapi/security/__init__.py` (lines 1–15, read at 4b3949cd9e) re-exports these 14 names:
- `APIKeyCookie`, `APIKeyHeader`, `APIKeyQuery`
- `HTTPAuthorizationCredentials`, `HTTPBasic`, `HTTPBasicCredentials`, `HTTPBearer`, `HTTPDigest`
- `OAuth2`, `OAuth2AuthorizationCodeBearer`, `OAuth2PasswordBearer`, `OAuth2PasswordRequestForm`, `OAuth2PasswordRequestFormStrict`, `SecurityScopes`
- `OpenIdConnect`

`SecurityBase`, `APIKeyBase` and `HTTPBase` are not re-exported.

**Inheritance:** `SecurityBase` is the common base. `APIKeyBase`, `HTTPBase`, `OAuth2` and `OpenIdConnect` extend it directly, and the concrete scheme classes extend those. The credentials, form and `SecurityScopes` classes are helpers and do not inherit from it.