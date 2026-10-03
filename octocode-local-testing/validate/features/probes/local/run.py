import json, subprocess, sys, os
D='/Users/bgaryy/code/octocode/octocode-local-testing/bench/validate/features/probes/local'
FX=D+'/fx'
def run(name, tool, q, env=None, raw=False, show=1200, args=None):
    """q: dict (single query) or list, or full envelope if raw"""
    if raw: payload=q
    elif isinstance(q, list): payload={'queries':q}
    else: payload=dict(goal='g',reasoning='r',**q) if 'followUp' not in q and 'goal' not in q else q
    e=dict(os.environ); e.update(env or {})
    cmd=['octocode',tool,json.dumps(payload)]+(args or [])
    p=subprocess.run(cmd,capture_output=True,text=True,env=e,cwd='/Users/bgaryy/code/octocode')
    open(f'{D}/{name}.out','w').write(f'# cmd: {" ".join(cmd[:2])} {json.dumps(payload)}\n# env: {env}\n# exit: {p.returncode}\n# stderr: {p.stderr[:2000]}\n{p.stdout}')
    print(f'[{name}] exit={p.returncode} bytes={len(p.stdout)} err={p.stderr[:200]!r}')
    if show: print(p.stdout[:show])
    try: return p.returncode, json.loads(p.stdout)
    except Exception: return p.returncode, p.stdout
