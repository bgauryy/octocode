# JSX (.jsx)

Source sample: `jsx/vite-app.jsx`

Strategy: `terser`

Agent rating: **8.9/10 (strong)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 3646 | - | - | - |
| content-view | 3645 | 0% | 1.359 ms | 8.3/10 |
| applyMinification | 2894 | 20.6% | 26.478 ms | 8.3/10 |
| sync minify | 2894 | 20.6% | 8.834 ms | 8.3/10 |
| async minify | 2894 | 20.6% | 6.705 ms | 8.3/10 |
| symbols | 246 | 93.3% | 0.658 ms | 10/10 |

## Notes

- engine-backed or parser-backed path.
- content-view kept original because the readable output was not shorter.

## Before Excerpt

```jsx
import { useState } from 'react'
import reactLogo from './assets/react.svg'
import viteLogo from './assets/vite.svg'
import heroImg from './assets/hero.png'
import './App.css'

function App() {
  const [count, setCount] = useState(0)

  return (
    <>
      <section id="center">
        <div className="hero">
          <img src={heroImg} className="base" width="170" height="179" alt="" />
          <img src={reactLogo} className="framework" alt="React logo" />
          <img src={viteLogo} className="vite" alt="Vite logo" />
        </div>
        <div>
          <h1>Get started</h1>
          <p>
            Edit <code>src/App.jsx</code> and save to test <code>HMR</code>
          </p>
        </div>
        <button
          type="button"
          className="counter"
          onClick={() => setCount((count) => count + 1)}
        >
          Count is {count}
        </button>
      </section>

      <div className="ticks"></div>

      <section id="next-steps">
        <div id="docs">
          <svg className="icon" role="presentation" aria-hidden="true">
            <use href="/icons.svg#documentation-icon"></use>
          </svg>
          <h2>Documentation</h2>
          <p>Your questions, answere

... [truncated 1846 chars] ...

   </a>
            </li>
            <li>
              <a href="https://bsky.app/profile/vite.dev" target="_blank">
                <svg
                  className="button-icon"
                  role="presentation"
                  aria-hidden="true"
                >
                  <use href="/icons.svg#bluesky-icon"></use>
                </svg>
                Bluesky
              </a>
            </li>
          </ul>
        </div>
      </section>

      <div className="ticks"></div>
      <section id="spacer"></section>
    </>
  )
}

export default App

```

## Content-View Excerpt

```jsx
import { useState } from 'react'
import reactLogo from './assets/react.svg'
import viteLogo from './assets/vite.svg'
import heroImg from './assets/hero.png'
import './App.css'

function App() {
  const [count, setCount] = useState(0)

  return (
    <>
      <section id="center">
        <div className="hero">
          <img src={heroImg} className="base" width="170" height="179" alt="" />
          <img src={reactLogo} className="framework" alt="React logo" />
          <img src={viteLogo} className="vite" alt="Vite logo" />
        </div>
        <div>
          <h1>Get started</h1>
          <p>
            Edit <code>src/App.jsx</code> and save to test <code>HMR</code>
          </p>
        </div>
        <button
          type="button"
          className="counter"
          onClick={() => setCount((count) => count + 1)}
        >
          Count is {count}
        </button>
      </section>

      <div className="ticks"></div>

      <section id="next-steps">
        <div id="docs">
          <svg className="icon" role="presentation" aria-hidden="true">
            <use href="/icons.svg#documentation-icon"></use>
          </svg>
          <h2>Documentation</h2>
          <p>Your questions, answere

... [truncated 1845 chars] ...

    </a>
            </li>
            <li>
              <a href="https://bsky.app/profile/vite.dev" target="_blank">
                <svg
                  className="button-icon"
                  role="presentation"
                  aria-hidden="true"
                >
                  <use href="/icons.svg#bluesky-icon"></use>
                </svg>
                Bluesky
              </a>
            </li>
          </ul>
        </div>
      </section>

      <div className="ticks"></div>
      <section id="spacer"></section>
    </>
  )
}

export default App
```

## Apply Minification Excerpt

```jsx
import{jsx as _jsx,jsxs as _jsxs,Fragment as _Fragment}from"react/jsx-runtime";import{useState}from"react";import reactLogo from"./assets/react.svg";import viteLogo from"./assets/vite.svg";import heroImg from"./assets/hero.png";import"./App.css";function App(){const[count,setCount]=useState(0);return _jsxs(_Fragment,{children:[_jsxs("section",{id:"center",children:[_jsxs("div",{className:"hero",children:[_jsx("img",{src:heroImg,className:"base",width:"170",height:"179",alt:""}),_jsx("img",{src:reactLogo,className:"framework",alt:"React logo"}),_jsx("img",{src:viteLogo,className:"vite",alt:"Vite logo"})]}),_jsxs("div",{children:[_jsx("h1",{children:"Get started"}),_jsxs("p",{children:["Edit ",_jsx("code",{children:"src/App.jsx"})," and save to test ",_jsx("code",{children:"HMR"})]})]}),_jsxs("button",{type:"button",className:"counter",onClick:()=>setCount(count=>count+1),children:["Count is ",count]})]}),_jsx("div",{className:"ticks"}),_jsxs("section",{id:"next-steps",children:[_jsxs("div",{id:"docs",children:[_jsx("svg",{className:"icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#documentation-icon"})}),_jsx("h2",{children:"Documentation"}),_jsx("p",{children:"Your ques

... [truncated 1094 chars] ...

cord"]})}),_jsx("li",{children:_jsxs("a",{href:"https://x.com/vite_js",target:"_blank",children:[_jsx("svg",{className:"button-icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#x-icon"})}),"X.com"]})}),_jsx("li",{children:_jsxs("a",{href:"https://bsky.app/profile/vite.dev",target:"_blank",children:[_jsx("svg",{className:"button-icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#bluesky-icon"})}),"Bluesky"]})})]})]})]}),_jsx("div",{className:"ticks"}),_jsx("section",{id:"spacer"})]})}export default App;
```

## Sync Minify Excerpt

```jsx
import{jsx as _jsx,jsxs as _jsxs,Fragment as _Fragment}from"react/jsx-runtime";import{useState}from"react";import reactLogo from"./assets/react.svg";import viteLogo from"./assets/vite.svg";import heroImg from"./assets/hero.png";import"./App.css";function App(){const[count,setCount]=useState(0);return _jsxs(_Fragment,{children:[_jsxs("section",{id:"center",children:[_jsxs("div",{className:"hero",children:[_jsx("img",{src:heroImg,className:"base",width:"170",height:"179",alt:""}),_jsx("img",{src:reactLogo,className:"framework",alt:"React logo"}),_jsx("img",{src:viteLogo,className:"vite",alt:"Vite logo"})]}),_jsxs("div",{children:[_jsx("h1",{children:"Get started"}),_jsxs("p",{children:["Edit ",_jsx("code",{children:"src/App.jsx"})," and save to test ",_jsx("code",{children:"HMR"})]})]}),_jsxs("button",{type:"button",className:"counter",onClick:()=>setCount(count=>count+1),children:["Count is ",count]})]}),_jsx("div",{className:"ticks"}),_jsxs("section",{id:"next-steps",children:[_jsxs("div",{id:"docs",children:[_jsx("svg",{className:"icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#documentation-icon"})}),_jsx("h2",{children:"Documentation"}),_jsx("p",{children:"Your ques

... [truncated 1094 chars] ...

cord"]})}),_jsx("li",{children:_jsxs("a",{href:"https://x.com/vite_js",target:"_blank",children:[_jsx("svg",{className:"button-icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#x-icon"})}),"X.com"]})}),_jsx("li",{children:_jsxs("a",{href:"https://bsky.app/profile/vite.dev",target:"_blank",children:[_jsx("svg",{className:"button-icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#bluesky-icon"})}),"Bluesky"]})})]})]})]}),_jsx("div",{className:"ticks"}),_jsx("section",{id:"spacer"})]})}export default App;
```

## Async Minify Excerpt

```jsx
import{jsx as _jsx,jsxs as _jsxs,Fragment as _Fragment}from"react/jsx-runtime";import{useState}from"react";import reactLogo from"./assets/react.svg";import viteLogo from"./assets/vite.svg";import heroImg from"./assets/hero.png";import"./App.css";function App(){const[count,setCount]=useState(0);return _jsxs(_Fragment,{children:[_jsxs("section",{id:"center",children:[_jsxs("div",{className:"hero",children:[_jsx("img",{src:heroImg,className:"base",width:"170",height:"179",alt:""}),_jsx("img",{src:reactLogo,className:"framework",alt:"React logo"}),_jsx("img",{src:viteLogo,className:"vite",alt:"Vite logo"})]}),_jsxs("div",{children:[_jsx("h1",{children:"Get started"}),_jsxs("p",{children:["Edit ",_jsx("code",{children:"src/App.jsx"})," and save to test ",_jsx("code",{children:"HMR"})]})]}),_jsxs("button",{type:"button",className:"counter",onClick:()=>setCount(count=>count+1),children:["Count is ",count]})]}),_jsx("div",{className:"ticks"}),_jsxs("section",{id:"next-steps",children:[_jsxs("div",{id:"docs",children:[_jsx("svg",{className:"icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#documentation-icon"})}),_jsx("h2",{children:"Documentation"}),_jsx("p",{children:"Your ques

... [truncated 1094 chars] ...

cord"]})}),_jsx("li",{children:_jsxs("a",{href:"https://x.com/vite_js",target:"_blank",children:[_jsx("svg",{className:"button-icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#x-icon"})}),"X.com"]})}),_jsx("li",{children:_jsxs("a",{href:"https://bsky.app/profile/vite.dev",target:"_blank",children:[_jsx("svg",{className:"button-icon",role:"presentation","aria-hidden":"true",children:_jsx("use",{href:"/icons.svg#bluesky-icon"})}),"Bluesky"]})})]})]})]}),_jsx("div",{className:"ticks"}),_jsx("section",{id:"spacer"})]})}export default App;
```

## Symbols

```txt
  1| import { useState } from 'react'
  2| import reactLogo from './assets/react.svg'
  3| import viteLogo from './assets/vite.svg'
  4| import heroImg from './assets/hero.png'
  5| import './App.css'
  7| function App() {
122| export default App
```
