The security schemes all share `SecurityBase`, defined in `fastapi/security/base.py`. The `__init__.py` and the file list below are read at 4b3949cd9e. The per-module class lists and line numbers come from a code search of the default branch (5f9fc5c), so I haven't confirmed them at the pinned commit.

**Modules in `fastapi/security/` at 4b3949cd9e**
- The directory holds `__init__.py`, `api_key.py`, `base.py`, `http.py`, `oauth2.py`, `open_id_connect_url.py` and `utils.py`.
- `__init__.py:1-15` re-exports the public names from `api_key`, `http`, `oauth2` and `open_id_connect_url` using `X as X` imports.
- It does not export `SecurityBase`, `APIKeyBase`, `HTTPBase` or anything from `utils.py`.

**Classes per module (default-branch line numbers)**
- **`base.py`**: `SecurityBase` (line 4).
- **`api_key.py`**:
  - `APIKeyBase(SecurityBase)` (line 11).
  - `APIKeyQuery`, `APIKeyHeader` and `APIKeyCookie` (lines 55, 147, 235), all subclasses of `APIKeyBase`.
- **`http.py`**:
  - `HTTPBasicCredentials(BaseModel)` (line 16) and `HTTPAuthorizationCredentials(BaseModel)` (line 29). These are pydantic models, not security schemes.
  - `HTTPBase(SecurityBase)` (line 69).
  - `HTTPBasic`, `HTTPBearer` and `HTTPDigest` (lines 105, 222, 319), all subclasses of `HTTPBase`.
- **`oauth2.py`**:
  - `OAuth2PasswordRequestForm` (line 14) and `OAuth2PasswordRequestFormStrict`, a subclass of the former (line 162). Both are plain form-dependency classes.
  - `OAuth2(SecurityBase)` (line 330).
  - `OAuth2PasswordBearer` and `OAuth2AuthorizationCodeBearer` (lines 433, 547), both subclasses of `OAuth2`.
  - `SecurityScopes` (line 653), a plain helper class.
- **`open_id_connect_url.py`**: `OpenIdConnect(SecurityBase)` (line 11).
- **`utils.py`**: the search found no classes in it. I didn't read the file, so I'm assuming it holds only helper functions.

**Inheritance**
- `SecurityBase` is the common base. `APIKeyBase`, `HTTPBase`, `OAuth2` and `OpenIdConnect` each subclass it directly, and the concrete schemes inherit from those.
- The credential models, the password request forms and `SecurityScopes` are not schemes, so they don't inherit from it.

**Uncertainty**
- I didn't read the bodies of the module files at 4b3949cd9e. If the repo changed between that commit and 5f9fc5c, the line numbers or class list could differ.