# HTML (.html)

Source sample: `html/00-mdn-letter.html`

Strategy: `aggressive`

Agent rating: **8.4/10 (strong)**

Agent understanding from minified output: **9.5/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 5096 | - | - | - |
| content-view | 5094 | 0% | 3.826 ms | 7.5/10 |
| applyMinification | 4615 | 9.4% | 4.235 ms | 7.5/10 |
| sync minify | 4615 | 9.4% | 4.185 ms | 7.5/10 |
| async minify | 4563 | 10.5% | 7.633 ms | 7.5/10 |
| symbols | 235 | 95.4% | 0.638 ms | 10/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 5/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 5096 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 5094 | 0% | 9.5/10 excellent | 10/10 | 10/10 |
| minify | 4563 | 10.5% | 9.8/10 excellent | 10/10 | 10/10 |
| symbols | 235 | 95.4% | 8.1/10 strong | 6.7/10 | 8/10 |

## Notes

- engine-backed or parser-backed path.
- content-view kept original because the readable output was not shorter.

## Before Excerpt

```html
<!DOCTYPE html>
<html lang="en-US">
  <head>
    <meta charset="utf-8">
    <meta name="author" content="Dr. Eleanor Gaye">
    <title>Awesome science application correspondence</title>
    <style>
      body {
        max-width: 800px;
        margin: 0 auto;
      }

      .sender-column {
        text-align: right;
      }

      h1 {
        font-size: 1.5em;
      }

      h2 {
        font-size: 1.3em;
      }

      p,ul,ol,dl,address {
        font-size: 1.1em;
      }

      p, li, dd, dt, address {
        line-height: 1.5;
      }
    </style>
  </head>
  <body>
    <address class="sender-column">
      <strong>Dr. Eleanor Gaye</strong><br>
      Awesome Science faculty<br>
      University of Awesome<br>
      Bobtown, CA 99999,<br>
      USA<br>
      <strong>Tel</strong>: 123-456-7890<br>
      <strong>Email</strong>: no_reply@example.com
    </address>

    <p class="sender-column"><time datetime="2016-01-20">20 January 2016</time></p>

    <address>
      <strong>Miss Eileen Dover</strong><br>
      4321 Cliff Top Edge<br>
      Dover, CT9 XXX<br>
      UK
    </address>

    <h1>Re: Eileen Dover university application</h1>

    <p>Dear Eileen,</p>

    <p>Thank you for your recent applica

... [truncated 3290 chars] ...

movements, being practiced by inhabitants of Northern Alaska and Canada. Later on however it was discovered that they were just moving like this because they were really cold.</dd>
    </dl>

    <p>For more of my research, see my <a href="http://www.example.com" aria-label="Dr Gaye's exotic dance research">exotic dance research page</a>.</p>

    <p>Yours sincerely,</p>

    <p>Dr Eleanor Gaye</p>


    <p>University of Awesome motto: <q>Be awesome to each other.</q> -- <cite>The memoirs of Bill S Preston, <abbr title="Esquire">Esq.</abbr></cite></p>
  </body>
</html>

```

## Content-View Excerpt

```html
<!DOCTYPE html>
<html lang="en-US">
  <head>
    <meta charset="utf-8">
    <meta name="author" content="Dr. Eleanor Gaye">
    <title>Awesome science application correspondence</title>
    <style>
      body {
        max-width: 800px;
        margin: 0 auto;
      }

      .sender-column {
        text-align: right;
      }

      h1 {
        font-size: 1.5em;
      }

      h2 {
        font-size: 1.3em;
      }

      p,ul,ol,dl,address {
        font-size: 1.1em;
      }

      p, li, dd, dt, address {
        line-height: 1.5;
      }
    </style>
  </head>
  <body>
    <address class="sender-column">
      <strong>Dr. Eleanor Gaye</strong><br>
      Awesome Science faculty<br>
      University of Awesome<br>
      Bobtown, CA 99999,<br>
      USA<br>
      <strong>Tel</strong>: 123-456-7890<br>
      <strong>Email</strong>: no_reply@example.com
    </address>

    <p class="sender-column"><time datetime="2016-01-20">20 January 2016</time></p>

    <address>
      <strong>Miss Eileen Dover</strong><br>
      4321 Cliff Top Edge<br>
      Dover, CT9 XXX<br>
      UK
    </address>

    <h1>Re: Eileen Dover university application</h1>

    <p>Dear Eileen,</p>

    <p>Thank you for your recent applica

... [truncated 3288 chars] ...

d movements, being practiced by inhabitants of Northern Alaska and Canada. Later on however it was discovered that they were just moving like this because they were really cold.</dd>
    </dl>

    <p>For more of my research, see my <a href="http://www.example.com" aria-label="Dr Gaye's exotic dance research">exotic dance research page</a>.</p>

    <p>Yours sincerely,</p>

    <p>Dr Eleanor Gaye</p>

    <p>University of Awesome motto: <q>Be awesome to each other.</q> -- <cite>The memoirs of Bill S Preston, <abbr title="Esquire">Esq.</abbr></cite></p>
  </body>
</html>
```

## Apply Minification Excerpt

```html
<!DOCTYPE html><html lang="en-US"><head><meta charset="utf-8"><meta name="author" content="Dr. Eleanor Gaye"><title>Awesome science application correspondence</title><style> body { max-width: 800px; margin: 0 auto; } .sender-column { text-align: right; } h1 { font-size: 1.5em; } h2 { font-size: 1.3em; } p,ul,ol,dl,address { font-size: 1.1em; } p, li, dd, dt, address { line-height: 1.5; } </style></head><body><address class="sender-column"><strong>Dr. Eleanor Gaye</strong><br> Awesome Science faculty<br> University of Awesome<br> Bobtown, CA 99999,<br> USA<br><strong>Tel</strong>: 123-456-7890<br><strong>Email</strong>: no_reply@example.com </address><p class="sender-column"><time datetime="2016-01-20">20 January 2016</time></p><address><strong>Miss Eileen Dover</strong><br> 4321 Cliff Top Edge<br> Dover, CT9 XXX<br> UK </address><h1>Re: Eileen Dover university application</h1><p>Dear Eileen,</p><p>Thank you for your recent application to join us at the University of Awesome's science faculty to study as part of your <abbr>PhD</abbr> (Doctor of Philosophy) next year. I will answer your questions one by one, in the following sections.</p><h2>Starting dates</h2><p>We are happy to accommodate you starting you

... [truncated 2809 chars] ...

haracterised by "robotic", stilted movements, being practiced by inhabitants of Northern Alaska and Canada. Later on however it was discovered that they were just moving like this because they were really cold.</dd></dl><p>For more of my research, see my <a href="http://www.example.com" aria-label="Dr Gaye's exotic dance research">exotic dance research page</a>.</p><p>Yours sincerely,</p><p>Dr Eleanor Gaye</p><p>University of Awesome motto: <q>Be awesome to each other.</q> -- <cite>The memoirs of Bill S Preston, <abbr title="Esquire">Esq.</abbr></cite></p></body></html>
```

## Sync Minify Excerpt

```html
<!DOCTYPE html><html lang="en-US"><head><meta charset="utf-8"><meta name="author" content="Dr. Eleanor Gaye"><title>Awesome science application correspondence</title><style> body { max-width: 800px; margin: 0 auto; } .sender-column { text-align: right; } h1 { font-size: 1.5em; } h2 { font-size: 1.3em; } p,ul,ol,dl,address { font-size: 1.1em; } p, li, dd, dt, address { line-height: 1.5; } </style></head><body><address class="sender-column"><strong>Dr. Eleanor Gaye</strong><br> Awesome Science faculty<br> University of Awesome<br> Bobtown, CA 99999,<br> USA<br><strong>Tel</strong>: 123-456-7890<br><strong>Email</strong>: no_reply@example.com </address><p class="sender-column"><time datetime="2016-01-20">20 January 2016</time></p><address><strong>Miss Eileen Dover</strong><br> 4321 Cliff Top Edge<br> Dover, CT9 XXX<br> UK </address><h1>Re: Eileen Dover university application</h1><p>Dear Eileen,</p><p>Thank you for your recent application to join us at the University of Awesome's science faculty to study as part of your <abbr>PhD</abbr> (Doctor of Philosophy) next year. I will answer your questions one by one, in the following sections.</p><h2>Starting dates</h2><p>We are happy to accommodate you starting you

... [truncated 2809 chars] ...

haracterised by "robotic", stilted movements, being practiced by inhabitants of Northern Alaska and Canada. Later on however it was discovered that they were just moving like this because they were really cold.</dd></dl><p>For more of my research, see my <a href="http://www.example.com" aria-label="Dr Gaye's exotic dance research">exotic dance research page</a>.</p><p>Yours sincerely,</p><p>Dr Eleanor Gaye</p><p>University of Awesome motto: <q>Be awesome to each other.</q> -- <cite>The memoirs of Bill S Preston, <abbr title="Esquire">Esq.</abbr></cite></p></body></html>
```

## Async Minify Excerpt

```html
<!DOCTYPE html><html lang="en-US"><head><meta charset="utf-8"><meta name="author" content="Dr. Eleanor Gaye"><title>Awesome science application correspondence</title><style>body{max-width:800px;margin:0 auto}.sender-column{text-align:right}h1{font-size:1.5em}h2{font-size:1.3em}address,dl,ol,p,ul{font-size:1.1em}address,dd,dt,li,p{line-height:1.5}</style></head><body><address class="sender-column"><strong>Dr. Eleanor Gaye</strong><br>Awesome Science faculty<br>University of Awesome<br>Bobtown, CA 99999,<br>USA<br><strong>Tel</strong>: 123-456-7890<br><strong>Email</strong>: no_reply@example.com</address><p class="sender-column"><time datetime="2016-01-20">20 January 2016</time></p><address><strong>Miss Eileen Dover</strong><br>4321 Cliff Top Edge<br>Dover, CT9 XXX<br>UK</address><h1>Re: Eileen Dover university application</h1><p>Dear Eileen,</p><p>Thank you for your recent application to join us at the University of Awesome's science faculty to study as part of your <abbr>PhD</abbr> (Doctor of Philosophy) next year. I will answer your questions one by one, in the following sections.</p><h2>Starting dates</h2><p>We are happy to accommodate you starting your study with us at any time, however it would suit u

... [truncated 2757 chars] ...

haracterised by "robotic", stilted movements, being practiced by inhabitants of Northern Alaska and Canada. Later on however it was discovered that they were just moving like this because they were really cold.</dd></dl><p>For more of my research, see my <a href="http://www.example.com" aria-label="Dr Gaye's exotic dance research">exotic dance research page</a>.</p><p>Yours sincerely,</p><p>Dr Eleanor Gaye</p><p>University of Awesome motto: <q>Be awesome to each other.</q> -- <cite>The memoirs of Bill S Preston, <abbr title="Esquire">Esq.</abbr></cite></p></body></html>
```

## Symbols

```txt
 1| <!DOCTYPE html>
 5|     <meta name="author" content="Dr. Eleanor Gaye">
54|     <h1>Re: Eileen Dover university application</h1>
60|     <h2>Starting dates</h2>
74|     <h2>Subjects of study</h2>
86|     <h2>Exotic dance moves</h2>
```
