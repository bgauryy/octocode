// Loopback HTML server for real-Chrome tests, started by page-server.mjs. argv[2] is JSON
// { page, routes }: a request url named in routes gets that HTML, every other url gets page.
// Prints the port it listens on.
const { page, routes } = JSON.parse(process.argv[2]);
const byUrl = new Map(Object.entries(routes));
require('node:http')
  .createServer((request, response) => {
    response.setHeader('content-type', 'text/html');
    response.end(byUrl.get(request.url) ?? page);
  })
  .listen(0, '127.0.0.1', function () {
    console.log(this.address().port);
  });
