# JSX (.jsx)

Source sample: `jsx/00-fullcalendar-demo.jsx`

Strategy: `terser`

Agent rating: **8.9/10 (strong)**

Agent understanding from minified output: **9.7/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 3825 | - | - | - |
| content-view | 3466 | 9.4% | 2.485 ms | 8.3/10 |
| applyMinification | 3025 | 20.9% | 57.469 ms | 8.3/10 |
| sync minify | 3025 | 20.9% | 27.718 ms | 8.3/10 |
| async minify | 3025 | 20.9% | 29.209 ms | 8.3/10 |
| symbols | 600 | 84.3% | 3.46 ms | 10/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 7/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 3825 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 3466 | 9.4% | 9.7/10 excellent | 10/10 | 10/10 |
| minify | 3025 | 20.9% | 8.5/10 strong | 6.7/10 | 10/10 |
| symbols | 600 | 84.3% | 8.2/10 strong | 6.7/10 | 8.6/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```jsx
import React, { useState } from 'react'
import { formatDate } from '@fullcalendar/core'
import FullCalendar from '@fullcalendar/react'
import dayGridPlugin from '@fullcalendar/daygrid'
import timeGridPlugin from '@fullcalendar/timegrid'
import interactionPlugin from '@fullcalendar/interaction'
import { INITIAL_EVENTS, createEventId } from './event-utils'

export default function DemoApp() {
  const [weekendsVisible, setWeekendsVisible] = useState(true)
  const [currentEvents, setCurrentEvents] = useState([])

  function handleWeekendsToggle() {
    setWeekendsVisible(!weekendsVisible)
  }

  function handleDateSelect(selectInfo) {
    let title = prompt('Please enter a new title for your event')
    let calendarApi = selectInfo.view.calendar

    calendarApi.unselect() // clear date selection

    if (title) {
      calendarApi.addEvent({
        id: createEventId(),
        title,
        start: selectInfo.startStr,
        end: selectInfo.endStr,
        allDay: selectInfo.allDay
      })
    }
  }

  function handleEventClick(clickInfo) {
    if (confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)) {
      clickInfo.event.remove()
    }
  }

  function handleEvents(events) {

... [truncated 2025 chars] ...

{handleWeekendsToggle}
          ></input>
          toggle weekends
        </label>
      </div>
      <div className='demo-app-sidebar-section'>
        <h2>All Events ({currentEvents.length})</h2>
        <ul>
          {currentEvents.map((event) => (
            <SidebarEvent key={event.id} event={event} />
          ))}
        </ul>
      </div>
    </div>
  )
}

function SidebarEvent({ event }) {
  return (
    <li key={event.id}>
      <b>{formatDate(event.start, {year: 'numeric', month: 'short', day: 'numeric'})}</b>
      <i>{event.title}</i>
    </li>
  )
}

```

## Content-View Excerpt

```jsx
import React, { useState } from 'react'
import { formatDate } from '@fullcalendar/core'
import FullCalendar from '@fullcalendar/react'
import dayGridPlugin from '@fullcalendar/daygrid'
import timeGridPlugin from '@fullcalendar/timegrid'
import interactionPlugin from '@fullcalendar/interaction'
import { INITIAL_EVENTS, createEventId } from './event-utils'

export default function DemoApp() {
  const [weekendsVisible, setWeekendsVisible] = useState(true)
  const [currentEvents, setCurrentEvents] = useState([])

  function handleWeekendsToggle() {
    setWeekendsVisible(!weekendsVisible)
  }

  function handleDateSelect(selectInfo) {
    let title = prompt('Please enter a new title for your event')
    let calendarApi = selectInfo.view.calendar

    calendarApi.unselect()

    if (title) {
      calendarApi.addEvent({
        id: createEventId(),
        title,
        start: selectInfo.startStr,
        end: selectInfo.endStr,
        allDay: selectInfo.allDay
      })
    }
  }

  function handleEventClick(clickInfo) {
    if (confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)) {
      clickInfo.event.remove()
    }
  }

  function handleEvents(events) {
    setCurrentEvents(ev

... [truncated 1666 chars] ...

={handleWeekendsToggle}
          ></input>
          toggle weekends
        </label>
      </div>
      <div className='demo-app-sidebar-section'>
        <h2>All Events ({currentEvents.length})</h2>
        <ul>
          {currentEvents.map((event) => (
            <SidebarEvent key={event.id} event={event} />
          ))}
        </ul>
      </div>
    </div>
  )
}

function SidebarEvent({ event }) {
  return (
    <li key={event.id}>
      <b>{formatDate(event.start, {year: 'numeric', month: 'short', day: 'numeric'})}</b>
      <i>{event.title}</i>
    </li>
  )
}
```

## Apply Minification Excerpt

```jsx
import{jsx as _jsx,jsxs as _jsxs,Fragment as _Fragment}from"react/jsx-runtime";import React,{useState}from"react";import{formatDate}from"@fullcalendar/core";import FullCalendar from"@fullcalendar/react";import dayGridPlugin from"@fullcalendar/daygrid";import timeGridPlugin from"@fullcalendar/timegrid";import interactionPlugin from"@fullcalendar/interaction";import{INITIAL_EVENTS,createEventId}from"./event-utils";export default function DemoApp(){const[weekendsVisible,setWeekendsVisible]=useState(!0),[currentEvents,setCurrentEvents]=useState([]);function handleWeekendsToggle(){setWeekendsVisible(!weekendsVisible)}function handleDateSelect(selectInfo){let title=prompt("Please enter a new title for your event"),calendarApi=selectInfo.view.calendar;calendarApi.unselect(),title&&calendarApi.addEvent({id:createEventId(),title:title,start:selectInfo.startStr,end:selectInfo.endStr,allDay:selectInfo.allDay})}function handleEventClick(clickInfo){confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)&&clickInfo.event.remove()}function handleEvents(events){setCurrentEvents(events)}return _jsxs("div",{className:"demo-app",children:[_jsx(Sidebar,{weekendsVisible:weekendsVisible,handleWeekendsTo

... [truncated 1225 chars] ...

r-section",children:_jsxs("label",{children:[_jsx("input",{type:"checkbox",checked:weekendsVisible,onChange:handleWeekendsToggle}),"toggle weekends"]})}),_jsxs("div",{className:"demo-app-sidebar-section",children:[_jsxs("h2",{children:["All Events (",currentEvents.length,")"]}),_jsx("ul",{children:currentEvents.map(event=>_jsx(SidebarEvent,{event:event},event.id))})]})]})}function SidebarEvent({event:event}){return _jsxs("li",{children:[_jsx("b",{children:formatDate(event.start,{year:"numeric",month:"short",day:"numeric"})}),_jsx("i",{children:event.title})]},event.id)}
```

## Sync Minify Excerpt

```jsx
import{jsx as _jsx,jsxs as _jsxs,Fragment as _Fragment}from"react/jsx-runtime";import React,{useState}from"react";import{formatDate}from"@fullcalendar/core";import FullCalendar from"@fullcalendar/react";import dayGridPlugin from"@fullcalendar/daygrid";import timeGridPlugin from"@fullcalendar/timegrid";import interactionPlugin from"@fullcalendar/interaction";import{INITIAL_EVENTS,createEventId}from"./event-utils";export default function DemoApp(){const[weekendsVisible,setWeekendsVisible]=useState(!0),[currentEvents,setCurrentEvents]=useState([]);function handleWeekendsToggle(){setWeekendsVisible(!weekendsVisible)}function handleDateSelect(selectInfo){let title=prompt("Please enter a new title for your event"),calendarApi=selectInfo.view.calendar;calendarApi.unselect(),title&&calendarApi.addEvent({id:createEventId(),title:title,start:selectInfo.startStr,end:selectInfo.endStr,allDay:selectInfo.allDay})}function handleEventClick(clickInfo){confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)&&clickInfo.event.remove()}function handleEvents(events){setCurrentEvents(events)}return _jsxs("div",{className:"demo-app",children:[_jsx(Sidebar,{weekendsVisible:weekendsVisible,handleWeekendsTo

... [truncated 1225 chars] ...

r-section",children:_jsxs("label",{children:[_jsx("input",{type:"checkbox",checked:weekendsVisible,onChange:handleWeekendsToggle}),"toggle weekends"]})}),_jsxs("div",{className:"demo-app-sidebar-section",children:[_jsxs("h2",{children:["All Events (",currentEvents.length,")"]}),_jsx("ul",{children:currentEvents.map(event=>_jsx(SidebarEvent,{event:event},event.id))})]})]})}function SidebarEvent({event:event}){return _jsxs("li",{children:[_jsx("b",{children:formatDate(event.start,{year:"numeric",month:"short",day:"numeric"})}),_jsx("i",{children:event.title})]},event.id)}
```

## Async Minify Excerpt

```jsx
import{jsx as _jsx,jsxs as _jsxs,Fragment as _Fragment}from"react/jsx-runtime";import React,{useState}from"react";import{formatDate}from"@fullcalendar/core";import FullCalendar from"@fullcalendar/react";import dayGridPlugin from"@fullcalendar/daygrid";import timeGridPlugin from"@fullcalendar/timegrid";import interactionPlugin from"@fullcalendar/interaction";import{INITIAL_EVENTS,createEventId}from"./event-utils";export default function DemoApp(){const[weekendsVisible,setWeekendsVisible]=useState(!0),[currentEvents,setCurrentEvents]=useState([]);function handleWeekendsToggle(){setWeekendsVisible(!weekendsVisible)}function handleDateSelect(selectInfo){let title=prompt("Please enter a new title for your event"),calendarApi=selectInfo.view.calendar;calendarApi.unselect(),title&&calendarApi.addEvent({id:createEventId(),title:title,start:selectInfo.startStr,end:selectInfo.endStr,allDay:selectInfo.allDay})}function handleEventClick(clickInfo){confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)&&clickInfo.event.remove()}function handleEvents(events){setCurrentEvents(events)}return _jsxs("div",{className:"demo-app",children:[_jsx(Sidebar,{weekendsVisible:weekendsVisible,handleWeekendsTo

... [truncated 1225 chars] ...

r-section",children:_jsxs("label",{children:[_jsx("input",{type:"checkbox",checked:weekendsVisible,onChange:handleWeekendsToggle}),"toggle weekends"]})}),_jsxs("div",{className:"demo-app-sidebar-section",children:[_jsxs("h2",{children:["All Events (",currentEvents.length,")"]}),_jsx("ul",{children:currentEvents.map(event=>_jsx(SidebarEvent,{event:event},event.id))})]})]})}function SidebarEvent({event:event}){return _jsxs("li",{children:[_jsx("b",{children:formatDate(event.start,{year:"numeric",month:"short",day:"numeric"})}),_jsx("i",{children:event.title})]},event.id)}
```

## Symbols

```txt
  1| import React, { useState } from 'react'
  2| import { formatDate } from '@fullcalendar/core'
  3| import FullCalendar from '@fullcalendar/react'
  4| import dayGridPlugin from '@fullcalendar/daygrid'
  5| import timeGridPlugin from '@fullcalendar/timegrid'
  6| import interactionPlugin from '@fullcalendar/interaction'
  7| import { INITIAL_EVENTS, createEventId } from './event-utils'
  9| export default function DemoApp() {
 81| function renderEventContent(eventInfo) {
 90| function Sidebar({ weekendsVisible, handleWeekendsToggle, currentEvents }) {
123| function SidebarEvent({ event }) {
```
