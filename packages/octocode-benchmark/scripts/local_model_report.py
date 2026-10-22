#!/usr/bin/env python3
"""Render local-model benchmark receipts without treating ungraded answers as correct."""
import argparse
import collections
import hashlib
import json
import statistics
from pathlib import Path


def candidate_answer(result):
    if result.get('answer', '').strip():
        return result['answer'], 'complete'
    trace = result.get('trace', [])
    if trace:
        reply = trace[-1]['inference']['result'].get('structuredContent', {}).get('response', {})
        message = reply.get('message', {})
        if message.get('content', '').strip() and not message.get('tool_calls'):
            return message['content'], 'partial'
    return '', 'none'


def collect(run):
    plan = json.loads((run / 'plan.json').read_text())
    grade_file = run / 'quality/grades.json'
    grades = json.loads(grade_file.read_text()) if grade_file.exists() else {}
    audit_file = run / 'quality/tool-audit.json'
    audits = json.loads(audit_file.read_text()).get('models', []) if audit_file.exists() else []
    review_file = run / 'quality/probe-review.json'
    probe_reviews = json.loads(review_file.read_text()) if review_file.exists() else {}
    fidelity_file = run / 'quality/native-fetch-fidelity.json'
    fidelity_rows = json.loads(fidelity_file.read_text()).get('questions', []) if fidelity_file.exists() else []
    fidelity = {(r['model'], r['question']): r for r in fidelity_rows}
    rows, models = [], []
    for model in plan['models']:
        trials = []
        for case in plan['cases']:
            file = run / 'trials' / model.replace(':', '_') / case['id'] / 'result.json'
            result = json.loads(file.read_text()) if file.exists() else {}
            answer, kind = candidate_answer(result)
            failures = [t['inference']['result'].get('structuredContent', {}).get('code')
                        for t in result.get('trace', []) if t['inference']['result'].get('isError')]
            grade = grades.get(model, {}).get(case['id']) if result else None
            marks = grade.get('marks') if grade else None
            if marks is not None and (len(marks) != 3 or any(x not in (0, 1, None) for x in marks)):
                raise ValueError('Expected three source checks for ' + model + '/' + case['id'])
            fetch = fidelity.get((model, case['id']))
            row = {'model': model, 'question': case['id'], 'title': case['question'].splitlines()[0].lstrip('# '),
                   'status': result.get('status', 'pending'), 'returnKind': kind,
                   'failureCode': next((f for f in failures if f), None),
                   'marks': marks, 'sourcePoints': sum(marks) if marks is not None and None not in marks else None,
                   'notes': grade.get('notes', '') if grade else '',
                   'fetchFidelity': {'status': fetch['status'], 'lines': fetch['linesChecked'],
                                     'mismatches': len(fetch['mismatches']), 'unverifiable': len(fetch['unverifiable'])} if fetch else None,
                   'rounds': result.get('rounds'), 'calls': result.get('toolCalls'), 'errors': result.get('toolErrors'),
                   'seconds': result.get('wallSeconds'), 'outputTokens': result.get('outputTokens'),
                   'answer': answer, 'sourceFile': str(file.relative_to(run)) if file.exists() else None}
            rows.append(row)
            if result:
                trials.append(result)
        checked = [r for r in rows if r['model'] == model and r['sourcePoints'] is not None]
        points = sum(r['sourcePoints'] for r in checked)
        usage = collections.Counter(e['tool'] for t in trials for turn in t['trace'] for e in turn['toolExecutions'])
        probe_file = run / 'probes' / (model.replace(':', '_') + '.json')
        probes = json.loads(probe_file.read_text()) if probe_file.exists() else []
        audit = next((a for a in audits if a['model'] == model), {})
        observations = [turn['inference']['result'].get('structuredContent', {}).get('execution', {}).get('observation', {})
                        for t in trials for turn in t['trace']]
        resident_sizes = [o['size'] for o in observations if isinstance(o.get('size'), (int, float))]
        models.append({'model': model, 'finished': len(trials), 'expected': len(plan['cases']),
                       'completeResponses': sum(t['status'] == 'answered' for t in trials),
                       'statuses': dict(collections.Counter(t['status'] for t in trials)),
                       'inferenceFailureCodes': dict(collections.Counter(r['failureCode'] for r in rows if r['model'] == model and r['failureCode'])),
                       'gradedQuestions': len(checked), 'sourcePointsChecked': points,
                       'fullySupportedQuestionsChecked': sum(r['sourcePoints'] == 3 for r in checked),
                       'sourcePoints': points if len(checked) == len(plan['cases']) else None,
                       'possiblePoints': 3 * len(plan['cases']), 'researchTools': dict(usage),
                       'nativeProbes': {p['expectedTool']: p['requestedToolSucceeded'] for p in probes},
                       'probeReview': probe_reviews.get(model, {}),
                       'toolAudit': audit.get('totals'),
                       'maxObservedResidentBytes': max(resident_sizes) if resident_sizes else None,
                       'probeSchema': {p['tool']: all(c['schemaConforms'] for c in p['calls']) for p in audit.get('probes', [])},
                       'seconds': sum(t['wallSeconds'] for t in trials),
                       'nativeSeconds': sum(e['wallSeconds'] for t in trials for turn in t['trace'] for e in turn['toolExecutions']),
                       'inferenceSeconds': sum(turn['inference']['wallSeconds'] for t in trials for turn in t['trace']),
                       'medianSeconds': statistics.median(t['wallSeconds'] for t in trials) if trials else None,
                       'rounds': sum(t['rounds'] for t in trials),
                       'toolErrors': sum(t['toolErrors'] for t in trials),
                       'returnedToolCharacters': sum(t['returnedToolCharacters'] for t in trials),
                       'outputTokensObserved': sum(t['outputTokens'] for t in trials if t['outputTokens'] is not None),
                       'outputUsageCoverage': sum(t['outputTokens'] is not None for t in trials)})
    ready = all(m['sourcePoints'] is not None and m['finished'] == m['expected'] for m in models)
    leaders = []
    if ready:
        maximum = max(m['sourcePoints'] for m in models)
        leaders = [m['model'] for m in models if m['sourcePoints'] == maximum]
    best = leaders[0] if len(leaders) == 1 else None
    return {'models': models, 'trials': rows, 'allQualityChecksComplete': ready,
            'descriptiveLeader': best, 'tiedLeaders': leaders if len(leaders) > 1 else [],
            'expectedTrials': len(plan['models']) * len(plan['cases']),
            'completedTrials': sum(m['finished'] for m in models)}


def prepare_review(run, data):
    plan = json.loads((run / 'plan.json').read_text())
    mapping = {}
    review = run / 'quality/review'
    review.mkdir(parents=True, exist_ok=True)
    for case in plan['cases']:
        ordered = sorted(plan['models'], key=lambda m: hashlib.sha256((case['id'] + m + '42').encode()).hexdigest())
        mapping[case['id']] = {chr(65 + i): m for i, m in enumerate(ordered)}
        candidates = []
        for label, model in mapping[case['id']].items():
            row = next(r for r in data['trials'] if r['model'] == model and r['question'] == case['id'])
            observations = []
            if row['sourceFile']:
                result = json.loads((run / row['sourceFile']).read_text())
                observations = [{'tool': e['tool'], 'arguments': e['arguments'], 'ok': e['ok'], 'result': e['result']}
                                for t in result['trace'] for e in t['toolExecutions']]
            candidates.append({'label': label, 'status': row['status'], 'returnKind': row['returnKind'],
                               'answer': row['answer'], 'observations': observations})
        (review / (case['id'] + '.json')).write_text(json.dumps({'question': case['question'], 'candidates': candidates}, indent=2))
    (run / 'quality/identity-map.json').write_text(json.dumps(mapping, indent=2))


def render(run, data):
    encoded = json.dumps(data, ensure_ascii=True).replace('<', '\\u003c')
    page = '''<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Local model comparison</title><style>
body{font:15px system-ui;background:#10141e;color:#edf2ff;margin:0;padding:28px;max-width:1500px}h1{font-size:26px}p{color:#b6c1d7}a{color:#95c7ff}.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(250px,1fr));gap:14px}.card{background:#1b2231;border:1px solid #34405a;border-radius:12px;padding:18px}.big{font-size:25px;font-weight:650}.bar{height:6px;background:#34405a;margin:12px 0}.fill{height:100%;background:#69aaff}table{border-collapse:collapse;width:100%;margin:18px 0;font-size:13px}th,td{text-align:left;padding:9px;border-bottom:1px solid #34405a;vertical-align:top}th{color:#b6c1d7}select,input{background:#202a3e;color:white;border:1px solid #526386;padding:8px;border-radius:6px;margin-right:8px}pre{white-space:pre-wrap;max-width:1000px;line-height:1.5}.pass{color:#7ce1b0}.fail{color:#ff9696}.pending{color:#aab8d1}details{max-width:1000px}small{color:#b6c1d7}
</style></head><body><h1>Local models × Octocode local tools</h1><p id="progress"></p><p>Source quality, response completion and tool execution are separate checks. A completed response is not a verified answer. Fetched-text checks verify returned lines, not relevance or completeness.</p><div class="cards" id="cards"></div><h2>Tool coverage</h2><p>Each cell shows native execution, supplied-schema conformance, reviewed target relevance, then model-selected research calls. This checks tool names, not every operation in every language.</p><table id="tools"></table><h2>Question results</h2><select id="model"><option value="">All models</option></select><select id="status"><option value="">All outcomes</option></select><input id="search" placeholder="Find a question"><table><thead><tr><th>Question / model</th><th>Outcome</th><th>Source checks</th><th>Rounds</th><th>Calls / errors</th><th>Seconds</th><th>Output tokens</th></tr></thead><tbody id="rows"></tbody></table><p>Initial prompt tokens are excluded. Adjusted input usage is unknown. Timings include managed inference and native calls; missing usage is not zero.</p><p><a href="README.md">Method and limitations</a> · <a href="report-summary.json">Measured data</a></p><script>
const data=__DATA__;
const esc=x=>String(x??'—').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const label={pending:'Pending',answered:'Response completed',emptyAnswer:'Empty response',generationBudgetExceeded:'Output limit',inferenceError:'Provider error',contextBudgetExceeded:'Context limit',budgetExhausted:'No final answer'};
document.querySelector('#progress').textContent=`${data.completedTrials} / ${data.expectedTrials} question trials finished. ${data.allQualityChecksComplete?'Source review complete.':'Source review pending; no quality winner yet.'}`;
const decision=document.createElement('p');decision.id='decision';document.querySelector('#progress').after(decision);
if(data.allQualityChecksComplete){const full=data.models.reduce((n,m)=>n+m.fullySupportedQuestionsChecked,0);decision.textContent=(data.descriptiveLeader?'Highest observed part score: '+data.descriptiveLeader+'. ':'No unique leader on supported parts. ')+(full===0?'No model fully answered all three parts of any question in this bounded flow.':'This is an exploratory result under one prompt and budget, not a general model ranking.');}
document.querySelector('#cards').innerHTML=data.models.map(m=>`<div class="card"><strong>${esc(m.model)}</strong><div class="bar"><div class="fill" style="width:${100*m.finished/m.expected}%"></div></div><div class="big">${m.sourcePoints===null?'Quality pending':m.sourcePoints+' / '+m.possiblePoints}</div><p>Source-supported checks</p><div>${m.completeResponses} complete responses · ${m.finished} trials finished</div><small>${m.gradedQuestions} questions graded · ${Math.round(m.seconds)} seconds observed</small></div>`).join('');
const names=['structureSearch','localSearch','localFetch','astSearch','lspSearch'];
document.querySelector('#tools').innerHTML='<tr><th>Tool</th>'+data.models.map(m=>'<th>'+esc(m.model)+'</th>').join('')+'</tr>'+names.map(n=>'<tr><td>'+n+'</td>'+data.models.map(m=>`<td class="${m.nativeProbes[n]===undefined?'pending':m.nativeProbes[n]?'pass':'fail'}">${m.nativeProbes[n]===undefined?'Pending':m.nativeProbes[n]?'Native OK':'Native failed'} / schema ${m.probeSchema[n]===undefined?'pending':m.probeSchema[n]?'OK':'failed'} / target ${m.probeReview[n]===undefined?'pending':m.probeReview[n].targetSatisfied?'OK':'failed'} · ${m.researchTools[n]||0} calls</td>`).join('')+'</tr>').join('');
for(const m of data.models)document.querySelector('#model').insertAdjacentHTML('beforeend',`<option>${esc(m.model)}</option>`);
for(const s of [...new Set(data.trials.map(t=>t.status))])document.querySelector('#status').insertAdjacentHTML('beforeend',`<option value="${esc(s)}">${esc(label[s]||s)}</option>`);
function fetchLabel(f){if(!f)return 'Fetched text: not checked';if(f.status==='not_exercised')return 'Fetched text: not exercised';return f.status==='verified_returned_lines'?'Fetched text: '+f.lines+' lines match':'Fetched text: '+f.mismatches+' differences, '+f.unverifiable+' unverified';}
function draw(){const m=document.querySelector('#model').value,s=document.querySelector('#status').value,q=document.querySelector('#search').value.toLowerCase();document.querySelector('#rows').innerHTML=data.trials.filter(t=>(!m||t.model===m)&&(!s||t.status===s)&&(!q||(t.question+' '+t.title).toLowerCase().includes(q))).map(t=>`<tr><td><details><summary>${esc(t.question)} · ${esc(t.title)}<br><small>${esc(t.model)}</small></summary><p>${esc(t.returnKind)} returned content</p><pre>${esc(t.answer||'No answer available')}</pre><p>${esc(t.notes)}</p></details></td><td>${esc(t.failureCode==='resource_admission_unavailable'?'Memory admission refused':label[t.status]||t.status)}</td><td>${t.sourcePoints===null?'Not graded':t.sourcePoints+' / 3'}<br><small>${esc(fetchLabel(t.fetchFidelity))}</small></td><td>${esc(t.rounds)}</td><td>${esc(t.calls)} / ${esc(t.errors)}</td><td>${t.seconds===null?'—':t.seconds.toFixed(1)}</td><td>${esc(t.outputTokens)}</td></tr>`).join('')}
for(const id of ['model','status','search'])document.querySelector('#'+id).addEventListener('input',draw);draw();
</script></body></html>'''.replace('__DATA__', encoded)
    (run / 'comparison.html').write_text(page)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--run', type=Path, required=True)
    ap.add_argument('--prepare-review', action='store_true')
    args = ap.parse_args()
    data = collect(args.run)
    (args.run / 'report-summary.json').write_text(json.dumps(data, indent=2))
    if args.prepare_review:
        prepare_review(args.run, data)
    render(args.run, data)
    print(json.dumps({'finished': data['completedTrials'], 'expected': data['expectedTrials'],
                      'qualityComplete': data['allQualityChecksComplete'], 'html': str(args.run / 'comparison.html')}))


if __name__ == '__main__':
    main()
