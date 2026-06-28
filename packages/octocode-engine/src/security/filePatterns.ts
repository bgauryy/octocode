// File-access block list: prevents reading sensitive files by name/extension.
// SYNC NOTE: discoveryFilter.ts:DISCOVERY_IGNORED_FILE_NAMES and
// DISCOVERY_IGNORED_FILE_EXTENSIONS partially overlap this list (e.g. id_rsa,
// id_dsa, .pem, .key, .crt). Both sets must be kept in sync — a new sensitive
// file added here should also be added to discoveryFilter.ts to prevent it
// from appearing in discovery results, and vice versa.
export const IGNORED_FILE_PATTERNS: RegExp[] = [
  /\.env$/,
  /\.env\..+$/,

  /^\.npmrc$/,
  /^\.pypirc$/,
  /^\.netrc$/,
  /^\.dockercfg$/,
  /^\.docker\/config\.json$/,
  /^credentials$/,
  /^\.credentials$/,
  /^\.aws\/credentials$/,
  /^\.aws\/config$/,

  /^\.ssh\/.*$/,
  /^id_rsa$/,
  /^id_dsa$/,
  /^id_ecdsa$/,
  /^id_ed25519$/,
  /^id_rsa\.pub$/,
  /^id_dsa\.pub$/,
  /^id_ecdsa\.pub$/,
  /^id_ed25519\.pub$/,
  /^known_hosts$/,
  /^authorized_keys$/,

  /.*_rsa$/,
  /.*_dsa$/,
  /.*_ecdsa$/,
  /.*_ed25519$/,

  /^private.*\.key$/,
  /^private.*\.pem$/,
  /^.*[-_]private[-_].*\.key$/,
  /^.*[-_]private[-_].*\.pem$/,
  /[-_]private\.key$/,
  /[-_]private\.pem$/,
  /^.*\.private\.key$/,
  /^.*\.private\.pem$/,

  /\.pem$/,
  /\.key$/,
  /\.crt$/,
  /\.cer$/,

  /\.keystore$/,
  /\.p12$/,
  /\.pfx$/,
  /\.jks$/,
  /\.ppk$/,

  /^service[-_]account.*\.json$/,
  /^.*-service[-_]account.*\.json$/,
  /^application[-_]default[-_]credentials\.json$/,
  /^gcloud[-_]credentials\.json$/,

  /^kubeconfig$/,
  /^\.kube\/config$/,
  /^k8s[-_]config$/,

  /^terraform\.tfstate$/,
  /^terraform\.tfstate\.backup$/,
  /^terraform\.tfvars$/,
  /^\.terraform\.lock\.hcl$/,

  /^\.pgpass$/,
  /^\.my\.cnf$/,
  /^\.psqlrc$/,

  /^secrets\.yml$/,
  /^master\.key$/,
  /^config\/master\.key$/,

  /^wp-config\.php$/,

  /^google-services\.json$/,
  /^GoogleService-Info\.plist$/,
  /^keystore\.properties$/,
  /\.mobileprovision$/,
  /\.provisionprofile$/,

  /^\.token$/,
  /^token\.txt$/,
  /^access_token$/,
  /^refresh_token$/,
  /^bearer_token$/,
  /^auth_token$/,

  /^client_secret.*\.json$/,
  /^oauth2.*\.json$/,

  /^\.password$/,
  /^password\.txt$/,
  /^passwords\.txt$/,
  /^\.secret$/,
  /^secret\.txt$/,
  /^secrets\.txt$/,
  /^apikey\.txt$/,
  /^api_key\.txt$/,
  /^api-key\.txt$/,

  /^\.git-credentials$/,
  /^git-credentials$/,

  /^\.htpasswd$/,

  /^auth\.json$/,

  /^\.ftpconfig$/,
  /^ftpconfig\.json$/,

  /^_netrc$/,

  /^\.bash_history$/,
  /^\.zsh_history$/,
  /^\.sh_history$/,
  /^\.history$/,

  /^\.mysql_history$/,
  /^\.psql_history$/,
  /^\.sqlite_history$/,

  /^\.pip\/pip\.conf$/,

  /^credentials\.xml$/,
  /^secrets\/.*\.xml$/,

  /^shadow$/,
  /^shadow\.bak$/,
  /^gshadow$/,

  /^\.muttrc$/,
  /^\.mailrc$/,

  /^\.ircrc$/,

  /^\.s3cfg$/,
  /^s3cfg$/,

  /^\.slack-token$/,
  /^slack[-_]token$/,

  /^\.github[-_]token$/,
  /^github[-_]token$/,

  /^\.netrc\.heroku$/,

  /^\.circleci\/local[-_]config\.yml$/,

  /^vault[-_]pass.*\.txt$/,
  /^\.vault[-_]pass.*$/,

  /^\.npm[-_]token$/,

  /^settings\.xml$/,
  /^\.m2\/settings\.xml$/,

  /^gradle\.properties$/,
  /^\.gradle\/gradle\.properties$/,

  /^\.subversion\/auth\/.*$/,

  /^Login Data$/,
  /^Cookies$/,
  /\/Login Data$/,
  /\/Cookies$/,
  /\/logins\.json$/,
  /\/key[34]\.db$/,
  /\.mozilla\/firefox\/.*\/logins\.json$/,
  /\.mozilla\/firefox\/.*\/key[34]\.db$/,
  /\.config\/chromium\/.*\/Login Data$/,
  /\.config\/google-chrome\/.*\/Login Data$/,

  /\.kdbx$/,
  /\.kdb$/,
  /^1Password\.sqlite$/,
  /^1Password.*\.sqlite$/,
  /\.agilekeychain\//,
  /\.opvault\//,
  /^password-store$/,
  /^passwords\.kdbx$/,
  /^keepass\.kdbx$/,

  /^wallet\.dat$/,
  /^default_wallet$/,
  /^\.bitcoin\/wallet\.dat$/,
  /^\.ethereum\/keystore\/.*$/,
  /^\.electrum\/wallets\/.*$/,
  /\/keystore\/UTC--.*$/,

  /^dump\.rdb$/,
  /^mongodb\.dump$/,
  /\.bson$/,
  /\.dump$/,

  /^\.idea\/dataSources\.xml$/,
  /^\.idea\/webServers\.xml$/,
  /^\.idea\/deployment\.xml$/,

  /^\.vagrant\/machines\/.*\/private_key$/,
  /^\.vagrant\.d\/insecure_private_key$/,

  /^cookies\.txt$/,
  /^\.cookies$/,
  /^session$/,
  /^sessionid$/,
  /^\.wget-hsts$/,

  /^core\.\d+$/,
  /\.core$/,
  /\.dmp$/,
  /\.mdmp$/,

  /^Credentials$/,
  /^NTUSER\.DAT$/,
  /^SAM$/,

  /\.keychain$/,
  /\.keychain-db$/,

  /\.csr$/,

  /^oauth[-_]token.*$/,
  /^bearer[-_]token.*$/,
  /^jwt[-_]token.*$/,
  /^\.token\..*$/,
  /^api[-_]keys?\..*$/,

  /^\.redis_history$/,
  /^\.mongo_history$/,
  /^\.dbshell$/,

  /^docker-compose\.override\.yml$/,
  /^docker-compose\..*\.yml$/,

  /^\.gcp[-_]credentials\.json$/,
  /^\.azure[-_]credentials$/,
  /^\.do[-_]token$/,
  /^\.linode[-_]token$/,

  /^\.msmtprc$/,
  /^\.fetchmailrc$/,

  /^wireguard\.conf$/,
  /^wg[0-9]+\.conf$/,
  /\.ovpn$/,

  /^PASSWORDS\.md$/,
  /^SECRETS\.md$/,
  /^CREDENTIALS\.md$/,
  /^ACCESS\.md$/,

  /\.asc$/,
  /\.gpg$/,

  /^\.ssh\/config$/,

  /\.rdp$/,

  /^credentials\.db$/,
  /^credentials\.sqlite$/,

  /\.postman_environment\.json$/,

  /^\.ruby-env$/,
  /^\.rbenv-vars$/,

  /^\.python-env$/,

  /^\.node-env$/,

  /^\.rsync[-_]password$/,
  /^rsync[-_]password$/,
];
