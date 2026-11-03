// Real build metadata for installed language servers; no protocol responses are synthesized.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

export function liveServerEnvironment(base, root) {
  const env = { ...base };
  const java = '/opt/homebrew/opt/openjdk@25/libexec/openjdk.jdk/Contents/Home';
  const dotnet = '/opt/homebrew/opt/dotnet/libexec';
  if (!env.JAVA_HOME && fs.existsSync(java)) env.JAVA_HOME = java;
  if (!env.DOTNET_ROOT && fs.existsSync(dotnet)) env.DOTNET_ROOT = dotnet;
  env.PATH = [path.join(root, 'node_modules', '.bin'), path.join(os.homedir(), '.local/share/octocode-e2e/bin'),
    ...(env.JAVA_HOME ? [path.join(env.JAVA_HOME, 'bin')] : []), base.PATH].join(path.delimiter);
  env.DOTNET_CLI_TELEMETRY_OPTOUT = '1';
  env.DOTNET_SKIP_FIRST_TIME_EXPERIENCE = '1';
  env.DOTNET_GENERATE_ASPNET_CERTIFICATE = 'false';
  return env;
}

export function configureRealServers(home) {
  const directory = home;
  fs.mkdirSync(directory, { recursive: true });
  const languageServers = Object.fromEntries(['asm', 's', 'assembly'].map(ext => [`.${ext}`, { command: 'asm-lsp', languageId: 'asm' }]));
  languageServers['.sbt'] = { command: 'metals', languageId: 'scala', initializationOptions: {
    metals: { autoImportBuilds: 'all', defaultBspToBuildTool: true },
  } };
  fs.writeFileSync(path.join(directory, 'lsp-servers.json'), JSON.stringify({ languageServers }));
}

export async function runProjectCommand(command, args, { cwd, env, report }) {
  const started = Date.now();
  const result = await new Promise(resolve => {
    const child = spawn(command, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'], timeout: 300_000 });
    let stdout = '', stderr = '';
    child.stdout.on('data', chunk => { stdout += chunk; });
    child.stderr.on('data', chunk => { stderr += chunk; });
    child.on('error', error => resolve({ error: error.message, stdout, stderr }));
    child.on('close', (status, signal) => resolve({ status, signal, stdout, stderr }));
  });
  report.setup.push({ command, args, cwd, ...result, ms: Date.now() - started });
  if (result.status !== 0) throw new Error(`${command} ${args.join(' ')} failed: ${JSON.stringify(result)}`);
  return result;
}

async function createScalaMetadata({ ext, file, cwd, write, run }) {
  if (ext !== 'sbt') {
    await run('scala-cli', ['compile', file, '--scala', '2.13.18', '--server=false', '--semanticdb']);
    await run('scala-cli', ['setup-ide', file, '--scala', '2.13.18', '--semanticdb']);
    return;
  }
  const version = await run('metals', ['--version']);
  const metalsVersion = version.stdout.match(/\bmetals ([\w.-]+)/)?.[1];
  if (!metalsVersion) throw new Error('Installed Metals did not report its version');
  fs.mkdirSync(path.join(cwd, 'project/project'), { recursive: true });
  write('project/build.properties', 'sbt.version=1.12.15\n');
  for (const directory of ['project', 'project/project']) write(`${directory}/metals.sbt`, `addSbtPlugin("org.scalameta" % "sbt-metals" % "${metalsVersion}")\n`);
  await run('sbt', ['--server', '--batch', 'compile', 'bspConfig']);
}

function projectFixture(language, ext, fixture) {
  if (language === 'Scala' && ext === 'sbt') return { ...fixture,
    src: 'val LIMIT = 3\n\ndef helper(n: Int): Int = n * 2\n\nname := {\n  val s = "😀"; helper(s.length).toString\n}\nscalaVersion := "2.13.18"\n' };
  if (language !== 'JavaScript') return fixture;
  // The exported parameter must be typed under the real server's strict JS
  // checking. Keep line positions; the oracle recalculates columns from source.
  let src = fixture.src.replace('function helper(n)', '/** @param {number} n */ function helper(n)');
  if (ext === 'mjs') src = src.replace('module.exports = { Widget, helper, run };', 'export { Widget, helper, run };');
  return { ...fixture, src };
}

function scriptConfig(language) {
  return { compilerOptions: { allowJs: true, checkJs: true, jsx: 'preserve', noEmit: true,
    ...(language === 'JavaScript' && { strict: true, noImplicitAny: true }) }, include: ['*'] };
}

export async function createLiveProject({ tmp, capability, ext, fixture, env, report }) {
  fixture = projectFixture(capability.language, ext, fixture);
  const cwd = path.join(tmp, ext);
  fs.mkdirSync(cwd);
  const file = path.join(cwd, capability.language === 'Java' ? 'Widget.java' : ext === 'sbt' ? 'build.sbt' : `main.${ext}`);
  fs.writeFileSync(file, fixture.src);
  const write = (name, text) => fs.writeFileSync(path.join(cwd, name), text);
  const run = (command, args) => runProjectCommand(command, args, { cwd, env, report });
  if (capability.language === 'Rust') write('Cargo.toml', '[package]\nname="grammar_fixture"\nversion="0.0.0"\nedition="2021"\n[lib]\npath="main.rs"\n');
  if (['TypeScript', 'TSX', 'JavaScript'].includes(capability.language)) write('tsconfig.json', JSON.stringify(scriptConfig(capability.language)));
  if (capability.language === 'Go') {
    write('go.mod', 'module example.com/grammar\n\ngo 1.22\n');
    await run('go', ['test', './...']);
  }
  if (capability.language === 'Java') {
    write('.project', '<?xml version="1.0"?><projectDescription><name>grammar-java</name><buildSpec><buildCommand><name>org.eclipse.jdt.core.javabuilder</name></buildCommand></buildSpec><natures><nature>org.eclipse.jdt.core.javanature</nature></natures></projectDescription>');
    write('.classpath', '<?xml version="1.0"?><classpath><classpathentry kind="src" path=""/><classpathentry kind="con" path="org.eclipse.jdt.launching.JRE_CONTAINER"/><classpathentry kind="output" path="bin"/></classpath>');
    await run('javac', ['-d', 'bin', file]);
  }
  if (capability.language === 'C#') {
    write('Grammar.csproj', '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>');
    await run('dotnet', ['build', '--nologo', '--disable-build-servers']);
  }
  if (capability.language === 'CUDA') write('compile_flags.txt', '-xcuda\n--cuda-host-only\n-nocudainc\n-nocudalib\n-D__device__=\n-D__global__=\n');
  if (capability.language === 'Assembly') {
    await run('git', ['init', '--quiet']);
    write('.asm-lsp.toml', '[default_config]\nassembler = "nasm"\ninstruction_set = "x86-64"\n[opts]\ndiagnostics = false\ndefault_diagnostics = false\n');
  }
  if (capability.language === 'Scala') await createScalaMetadata({ ext, file, cwd, write, run });
  return { cwd, file, fixture };
}
