The security schemes all share `SecurityBase`, defined in `fastapi/security/base.py:4`. The module list below is from the pinned commit. The class lists come from a code search that ran against the default branch (`5f9fc5c5`), not `4b3949cd9e`, so I didn't confirm them at the pinned commit. The `__init__.py` re-exports at the pinned commit are consistent with them.

**Modules** (`fastapi/security/` at `4b3949cd9e`): `__init__.py`, `api_key.py`, `base.py`, `http.py`, `oauth2.py`, `open_id_connect_url.py`, `utils.py`.

**Public classes per module:**
- **`base.py`**: `SecurityBase`. It is a plain class that only declares `model: SecurityBaseModel` and `scheme_name: str` (`base.py:4-6`). `SecurityBaseModel` is imported from `fastapi.openapi.models`.
- **`api_key.py`**:
  - `APIKeyBase(SecurityBase)`, the intermediate base class.
  - `APIKeyQuery`, `APIKeyHeader` and `APIKeyCookie`, each subclassing `APIKeyBase`.
- **`http.py`**:
  - `HTTPBasicCredentials` and `HTTPAuthorizationCredentials`, which are pydantic `BaseModel`s and not security schemes.
  - `HTTPBase(SecurityBase)`.
  - `HTTPBasic`, `HTTPBearer` and `HTTPDigest`, each subclassing `HTTPBase`.
- **`oauth2.py`**:
  - `OAuth2PasswordRequestForm` and its subclass `OAuth2PasswordRequestFormStrict`. These are form-data dependency classes, not schemes.
  - `OAuth2(SecurityBase)`.
  - `OAuth2PasswordBearer` and `OAuth2AuthorizationCodeBearer`, both subclassing `OAuth2`.
  - `SecurityScopes`, a helper class that is not a scheme.
- **`open_id_connect_url.py`**: `OpenIdConnect(SecurityBase)`.
- **`utils.py`**: I did not read it. The search found no classes in it, so it probably holds helper functions only.

**`__init__.py`** re-exports 14 names (`__init__.py:1-15`):
- `APIKeyCookie`, `APIKeyHeader`, `APIKeyQuery`
- `HTTPAuthorizationCredentials`, `HTTPBasic`, `HTTPBasicCredentials`, `HTTPBearer`, `HTTPDigest`
- `OAuth2`, `OAuth2AuthorizationCodeBearer`, `OAuth2PasswordBearer`, `OAuth2PasswordRequestForm`, `OAuth2PasswordRequestFormStrict`, `SecurityScopes`
- `OpenIdConnect`

It does not re-export `SecurityBase`, `APIKeyBase` or `HTTPBase`.

**Hierarchy:** every scheme derives from `SecurityBase`, either directly (`OAuth2`, `OpenIdConnect`) or through `APIKeyBase` or `HTTPBase`.