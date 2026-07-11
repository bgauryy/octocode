import type { ParsedArgs } from '../../types.js';
import { c, bold, dim } from '../../../utils/colors.js';
import { EXIT } from '../../exit-codes.js';
import {
  login as oauthLogin,
  logout as oauthLogout,
  getAuthStatusAsync,
  getStoragePath,
  type VerificationInfo,
} from '../../../features/github-oauth.js';
import { Spinner } from '../../../utils/spinner.js';
import {
  hasEnvToken,
  getEnvTokenSource,
} from '../../../utils/token-storage.js';
import {
  isOctocodeAuthStatus,
  normalizeGitProtocol,
  printInvalidGitProtocol,
} from './helpers.js';

export async function runOctocodeLogin(args: ParsedArgs): Promise<void> {
  const hostnameOpt = args.options['hostname'];
  const hostname =
    (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) || 'github.com';
  const jsonOutput = Boolean(args.options['json']);
  const forceLogin = Boolean(args.options['force']);
  const status = await getAuthStatusAsync(hostname);
  const alreadyOctocode = isOctocodeAuthStatus(status);

  if (alreadyOctocode && !forceLogin) {
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: true,
          username: status.username || null,
          error: null,
          alreadyAuthenticated: true,
          tokenSource: 'octocode',
        })
      );
      return;
    }
    console.log();
    console.log(
      `  ${c('green', '✓')} Already authenticated as ${c('cyan', status.username || 'unknown')}`
    );
    console.log();
    console.log(`  ${dim('To switch accounts, use --force:')}`);
    console.log(`    ${c('cyan', '→')} ${c('yellow', 'login --force')}`);
    console.log();
    return;
  }

  if (forceLogin && alreadyOctocode) {
    if (!jsonOutput) {
      console.log();
      console.log(
        `  ${dim('Signing out')} ${c('cyan', status.username || hostname)} ${dim('before re-authenticating...')}`
      );
    }
    await oauthLogout(hostname);
  }

  if (!process.stdout.isTTY) {
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: false,
          error:
            'Login requires browser interaction. Run "login" in an interactive terminal.',
          requiresInteraction: true,
        })
      );
    } else {
      console.log();
      console.log(`  ${c('red', '✗')} Login requires browser interaction.`);
      console.log(
        `  ${dim('Run')} ${c('yellow', 'login')} ${dim('in an interactive terminal.')}`
      );
      console.log();
    }
    process.exitCode = EXIT.USAGE;
    return;
  }

  const gitProtocolOpt = args.options['git-protocol'];
  const gitProtocol = normalizeGitProtocol(gitProtocolOpt);
  if (!gitProtocol) {
    printInvalidGitProtocol(String(gitProtocolOpt), jsonOutput);
    return;
  }

  if (hasEnvToken()) {
    const envSource = getEnvTokenSource();
    const envVar = envSource?.replace('env:', '') || 'environment variable';
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          step: 'warning',
          envVar,
          message: `${envVar} is set and takes priority — stored OAuth token won't be used until you unset it`,
        })
      );
    } else {
      console.log();
      console.log(
        `  ${c('yellow', '⚠')} ${bold(envVar)} is set and takes priority over stored credentials.`
      );
      console.log(
        `  ${dim("Your new OAuth token will be stored but won't be used until you unset that variable.")}`
      );
    }
  }

  if (!jsonOutput) {
    console.log();
    console.log(`  ${bold('🔐 GitHub Authentication')}`);
    console.log();
  }

  let verificationShown = false;
  const spinner = jsonOutput
    ? null
    : new Spinner('Waiting for GitHub authentication...').start();

  const result = await oauthLogin({
    hostname,
    gitProtocol,
    onVerification: (verification: VerificationInfo) => {
      spinner?.stop();
      verificationShown = true;
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            step: 'verification',
            userCode: verification.user_code,
            verificationUri: verification.verification_uri,
            expiresIn: verification.expires_in,
          })
        );
      } else {
        console.log(
          `  ${c('yellow', '!')} First copy your one-time code: ${bold(verification.user_code)}`
        );
        console.log();
        console.log(
          `  ${bold('Opening')} ${c('cyan', verification.verification_uri)} ${bold('in your browser...')}`
        );
        console.log();
        console.log(`  ${dim('Waiting for authentication...')}`);
      }
    },
  });

  if (!verificationShown) {
    spinner?.stop();
  }

  if (jsonOutput) {
    console.log(
      JSON.stringify({
        step: 'result',
        success: result.success,
        username: result.username || null,
        error: result.error || null,
      })
    );
    if (!result.success) process.exitCode = EXIT.AUTH;
    return;
  }

  console.log();
  if (result.success) {
    console.log(`  ${c('green', '✓')} Authentication complete!`);
    console.log(
      `  ${c('green', '✓')} Signed in as ${c('cyan', result.username || 'unknown')}`
    );
    console.log();
    console.log(`  ${dim('Credentials stored in:')} ${getStoragePath()}`);
  } else {
    console.log(
      `  ${c('red', '✗')} Authentication failed: ${result.error || 'Unknown error'}`
    );
    process.exitCode = EXIT.AUTH;
  }
  console.log();
}
