#!/usr/bin/env node
const args = process.argv.slice(2);
const take = (flag) => {
  const i = args.indexOf(flag);
  return i >= 0 ? args[i + 1] : '';
};
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: schema-helper.mjs --intent "extract pricing and features"');
  process.exit(0);
}
const intent = take('--intent').trim();
if (!intent) {
  console.error(JSON.stringify({ ok: false, error: '--intent required' }, null, 2));
  process.exit(2);
}
const templates = [
  ['pricing', /price|pricing|plans?|features?|billing/, ['plan_name', 'price', 'billing_period', 'features', 'limits', 'cta_url']],
  ['products', /products?|catalog|items?|skus?/, ['product_name', 'description', 'price', 'features', 'image_url', 'product_url']],
  ['docs-endpoints', /api|endpoint|docs?|reference|curl/, ['endpoint_name', 'method', 'url_path', 'parameters', 'response_fields', 'code_example']],
  ['blog-posts', /blog|articles?|posts?/, ['title', 'author', 'published_date', 'summary', 'tags', 'url']],
  ['contacts', /contact|email|phone|address|support/, ['name', 'role', 'email', 'phone', 'address', 'contact_url']],
  ['jobs', /jobs?|careers?|hiring|roles?/, ['job_title', 'location', 'department', 'employment_type', 'apply_url']],
  ['changelog', /changelog|release|versions?|updates?/, ['version', 'date', 'changes', 'breaking_changes', 'url']],
  ['faq', /faq|questions?|answers?/, ['question', 'answer', 'category']],
  ['events', /events?|webinars?|conference/, ['event_name', 'date', 'location', 'description', 'registration_url']],
  ['tables', /tables?|rows?|columns?/, ['table_title', 'headers', 'rows', 'source_section']]
];
const match = templates.find(([, rx]) => rx.test(intent.toLowerCase())) || templates[0];
const [intentType, , fields] = match;
console.log(JSON.stringify({
  ok: true,
  intent,
  intentType,
  recommendedMode: 'extract',
  extractProperties: fields.join(', '),
  validation: { required: [fields[0]], optional: fields.slice(1) },
  citation: 'Validate extracted rows against text/*.clean.part-*.md or raw audit when disputed.'
}, null, 2));
