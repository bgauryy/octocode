import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { catastrophicCommand } from '../src/permission/shell.js';
import { tmp } from './helpers.js';

describe('bash guard command substitutions', () => {
  it('blocks substitutions in unquoted heredoc bodies even when nothing pipes them to a shell', () => {
    for (const command of [
      'cat <<EOF\n$(rm -rf /)\nEOF',
      'cat <<EOF\nhello `rm -rf ~` world\nEOF',
      'cat > notes.md <<EOF\nline\n$(echo $(mkfs.ext4 /dev/sda))\nEOF',
      'cat <<-EOF\n\t$(shutdown -h now)\n\tEOF',
      'cat <<EOF\n$(cat <<INNER\n$(reboot)\nINNER\n)\nEOF',
      'cat <<A <<B\nfirst\nA\n$(rm -rf /)\nB',
      'cat <<EOF\n$(rm -rf /)',
      // Double quotes inside a heredoc body are literal; the substitution still runs.
      'cat <<EOF\n"$(rm -rf /)"\nEOF',
    ]) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
  });

  it('blocks substitutions inside double-quoted strings', () => {
    for (const command of ['echo "$(rm -rf ~)"', 'echo "x `rm -rf /` y"', 'git commit -m "$(shutdown -h now)"']) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
  });

  it('keeps literal text allowed: quoted heredoc delimiters, single quotes, escapes and harmless substitutions', () => {
    for (const command of [
      "cat <<'EOF'\n$(rm -rf /)\nEOF",
      'cat <<"EOF"\n$(rm -rf /)\nEOF',
      "cat <<-'EOF'\n\t`rm -rf ~`\n\tEOF",
      "echo '$(rm -rf /)'",
      "echo '`rm -rf ~`'",
      'cat <<EOF\n\\$(rm -rf /)\nEOF',
      'cat <<EOF\n$(date) and `whoami`\nEOF',
      'cat <<EOF\nrm -rf /\nEOF',
      'echo "$(date)"',
      'echo "\\$(rm -rf /)"',
    ]) {
      expect(catastrophicCommand(command), command).toBeUndefined();
    }
  });
});

afterEach(() => {
  vi.unstubAllEnvs();
});

describe('destructive commands resolved against the shell folder', () => {
  const home = () => {
    const dir = tmp('octocode-guard-home-');
    vi.stubEnv('HOME', dir);
    return dir;
  };

  it('flags relative targets that reach home or a folder holding it', () => {
    const dir = home();
    const sub = path.join(dir, 'proj');
    for (const [command, cwd] of [
      ['rm -rf ..', sub],
      ['rm -rf ../', sub],
      ['rm -rf *', dir],
      ['rm -rf .', dir],
      ['rm -rf ./', dir],
      ['cd ~ && rm -rf ./', sub],
      ['cd && rm -rf *', sub],
      ['cd .. && rm -rf .', sub],
      ['sh -c "cd ~ && rm -rf ."', sub],
      ['cd ~; find . -delete', sub],
      ['rm -rf ../..', path.join(sub, 'a')],
      ['rm -rf ..', path.dirname(dir)],
    ] as const) {
      expect(catastrophicCommand(command, cwd), `${command} in ${cwd}`).toBeDefined();
    }
  });

  it('leaves relative targets inside the project alone, and needs a folder to resolve them', () => {
    const dir = home();
    const sub = path.join(dir, 'proj');
    for (const [command, cwd] of [
      ['rm -rf *', sub],
      ['rm -rf ./dist', dir],
      ['rm -rf .', sub],
      ['rm -rf node_modules ../sibling', sub],
      ['cd proj && rm -rf .', dir],
      ['cd /tmp && rm -rf *', sub],
      // The folder is unknown after `cd $DIR` or `cd -`: variable tricks are out of scope.
      ['cd $DIR && rm -rf *', dir],
      ['cd - && rm -rf *', dir],
    ] as const) {
      expect(catastrophicCommand(command, cwd), `${command} in ${cwd}`).toBeUndefined();
    }
    expect(catastrophicCommand('rm -rf ..')).toBeUndefined();
  });

  it('flags chmod, chown and chgrp -R on protected roots only', () => {
    const dir = home();
    for (const command of ['chmod -R 000 /', 'chown -R root ~', 'chmod -R 777 /etc', 'chgrp --recursive staff /usr/local', `chmod -R go-rwx ${dir}/.ssh`, 'sudo chown -R me:me /']) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
    for (const command of ['chmod 000 /', 'chmod -R 755 ./dist', 'chown -R me ~/project', 'chmod -R +x /tmp/x']) {
      expect(catastrophicCommand(command, path.join(dir, 'proj')), command).toBeUndefined();
    }
  });

  it('flags writes to disk devices and removals under /usr, /etc and ~/.ssh', () => {
    home();
    for (const command of [
      '> /dev/sda',
      'echo x >/dev/nvme0n1',
      'cat image.iso > /dev/disk2',
      'cat image.iso >> /dev/sdb1',
      'dd if=image.iso of=/dev/sda bs=1m',
      'cat x | tee /dev/rdisk2',
      'cp image.iso /dev/sdc',
      'shred /dev/sda',
      'rm /etc/hosts',
      'rm -rf /usr/local/lib',
      'rmdir /etc/foo.d',
      'rm ~/.ssh/id_ed25519',
      'rm -rf $HOME/.ssh',
      'sudo rm /usr/bin/env',
      'find /etc -name "*.bak" -delete',
    ]) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
    for (const command of ['echo x > /dev/null', 'cat /dev/sda > disk.img', 'cp /dev/sda disk.img', 'dd if=/dev/sda of=disk.img', 'ls /etc', 'cat /etc/hosts', 'rm /tmp/x', 'rm -rf ~/projects/old']) {
      expect(catastrophicCommand(command), command).toBeUndefined();
    }
  });

  it('uses the platform home for `~`', () => {
    const dir = home();
    expect(os.homedir()).toBe(dir);
    expect(catastrophicCommand('rm -rf ~')).toBeDefined();
  });
});

describe('bash safety', () => {
  it('names catastrophic commands and lets ordinary ones through', () => {
    for (const command of ['rm -rf /', 'sudo rm -fr /*', 'cd x && rm -r -f ~/', 'rm --recursive --force "$HOME"', 'rm -rf --no-preserve-root /', 'mkfs.ext4 /dev/sda1', 'echo ok; shutdown -h now', 'dd if=/dev/zero of=/dev/disk2', ':(){ :|:& };:', 'x=$(reboot)', 'echo "$(rm -rf /)"', 'echo "now `shutdown -h now`"', 'echo "a $(echo "$(mkfs.ext4 /dev/sda)")"']) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
    for (const command of ['rm -rf dist', 'rm -rf ./build/', 'rm -rf /tmp/octocode-test', 'rm ~/notes.txt', 'echo "rm -rf /"', 'git commit -m "reboot the tests"', 'dd if=a of=/dev/null', 'yarn build']) {
      expect(catastrophicCommand(command), command).toBeUndefined();
    }
  });

  it('ignores quoted text and heredoc bodies, checks dd per command and sees past sudo options', () => {
    for (const command of [
      'rm -rf ./dist',
      'rm -rf node_modules',
      'dd if=x of=/dev/null',
      'echo "rm -rf /"',
      'grep shutdown src',
      'git commit -m "fix: close the socket (shutdown path)"',
      "git commit -m 'reboot; halt (later)'",
      'cat > notes.md <<\'EOF\'\ngrep -rn "of=/dev/sda" docs\nrm -rf /\nEOF',
      'cat <<-EOF > x.sh\n\tshutdown -h now\n\tEOF\necho done',
      'grep -rn "of=/dev/sda" docs',
      'echo ":(){ :|:& };:"',
    ]) {
      expect(catastrophicCommand(command), command).toBeUndefined();
    }
    for (const command of [
      'rm -rf /',
      'rm -rf ~',
      'sudo rm -rf /',
      'sudo -u root rm -rf /',
      'doas -u root rm -rf /',
      'x && rm -rf $HOME',
      'rm -rf "/"',
      'mkfs.ext4 /dev/sda',
      'dd if=/dev/zero of=/dev/sda',
      'sudo dd if=/dev/zero of="/dev/sda" bs=1M',
      'cat <<EOF > x\nbody\nEOF\nrm -rf /',
      'echo "a (b"; reboot',
    ]) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
  });
});

describe('bash safety: what hides a catastrophic command', () => {
  // A script handed to a shell or eval is a command too.
  it.each([
    "bash -c 'rm -rf /'",
    'sh -c "cd /tmp; rm -rf ~"',
    "zsh -lc 'mkfs.ext4 /dev/sda'",
    'dash -ec "reboot"',
    "ksh -c 'echo hi && shutdown -h now'",
    'eval "rm -rf /"',
    'eval rm -rf "$HOME"',
    "bash -c \"sh -c 'rm -rf /'\"",
  ])('blocks a script a shell or eval runs: %s', (command) => {
    expect(catastrophicCommand(command)).toEqual(expect.any(String));
  });

  // Heredoc and herestring bodies fed to a shell, directly or through a pipe.
  it.each([
    'bash <<EOF\nrm -rf /\nEOF',
    "sh <<'X'\necho start\nrm -rf ~\nX",
    'cat <<EOF | bash\nrm -rf /\nEOF',
    'bash <<< "rm -rf /"',
    'echo "rm -rf /" | sh',
    "printf 'reboot' | bash -s",
  ])('blocks a heredoc or herestring fed to a shell: %s', (command) => {
    expect(catastrophicCommand(command)).toEqual(expect.any(String));
  });

  // Wrappers, their option values and leading operands.
  it.each([
    'timeout 5 rm -rf /',
    'timeout -s KILL 10 rm -rf /',
    'nice -n 10 rm -rf ~',
    'nice -5 rm -rf /',
    'ionice -c 3 rm -rf /',
    'stdbuf -o 0 rm -rf /',
    'caffeinate -t 60 rm -rf /',
    'chrt 10 rm -rf /',
    'taskset 0x3 rm -rf /',
    'xargs -I {} rm -rf /',
    'xargs -n 1 rm -rf ~',
  ])('blocks a command behind a wrapper: %s', (command) => {
    expect(catastrophicCommand(command)).toEqual(expect.any(String));
  });

});

describe('bash safety: targets and erasers', () => {
  // Targets that land on /, home or a folder holding home.
  it.each([
    'rm -rf ~/.',
    'rm -rf ~/',
    'rm -rf $HOME/..',
    'rm -rf ${HOME}/../',
    'rm -rf /usr/..',
    'rm -rf /./',
    'rm -rf //',
  ])('blocks a target that lands on /, home or a folder holding home: %s', (command) => {
    expect(catastrophicCommand(command)).toEqual(expect.any(String));
  });

  // Disk erasers.
  it.each([
    'diskutil eraseDisk APFS X disk2',
    'diskutil eraseVolume APFS X disk2s1',
    'diskutil zeroDisk disk2',
    'newfs_apfs /dev/disk2',
    'find / -name "*.log" -delete',
    'find ~ -type f -delete',
    'find -L / -exec rm {} +',
  ])('blocks a disk eraser: %s', (command) => {
    expect(catastrophicCommand(command)).toEqual(expect.any(String));
  });

  it('blocks a target that is the folder holding home', () => {
    expect(catastrophicCommand(`rm -rf ${path.posix.dirname(os.homedir().split(path.sep).join('/'))}`)).toEqual(expect.any(String));
  });

  it.each([
    'rm -rf node_modules',
    'rm -rf ~/projects/app/node_modules',
    'rm -rf $HOME/.cache/yarn',
    'find . -delete',
    'find . -name "*.tmp" -delete',
    'find / -name "*.log" -print',
    "bash -c 'yarn build && rm -rf dist'",
    'bash script.sh',
    'bash -c "echo rm -rf /"',
    'sh <<EOF\necho hello\nEOF',
    'echo "rm -rf /" > notes.txt',
    'echo "rm -rf /" | grep rm',
    'timeout 5 yarn test',
    'nice -n 10 make',
    'xargs -I {} rm -rf {}',
    'diskutil list',
    'eval "$(ssh-agent -s)"',
  ])('allows the scoped or inert command %s', (command) => {
    expect(catastrophicCommand(command)).toBeUndefined();
  });
});
