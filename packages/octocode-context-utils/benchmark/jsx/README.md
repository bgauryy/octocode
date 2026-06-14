# JSX (.jsx)

Source sample: `jsx/00-fullcalendar-demo.jsx`

Strategy: `terser`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 3825 | - | - |
| content-view | 2863 | 25.2% | 0.13 ms |
| applyMinification | 2863 | 25.2% | 0.053 ms |
| sync minify | 2863 | 25.2% | 0.036 ms |
| async minify | 2863 | 25.2% | 0.058 ms |
| symbols | 600 | 84.3% | 1.492 ms |

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
import React,{useState}from"react";import{formatDate}from"@fullcalendar/core";import FullCalendar from"@fullcalendar/react";import dayGridPlugin from"@fullcalendar/daygrid";import timeGridPlugin from"@fullcalendar/timegrid";import interactionPlugin from"@fullcalendar/interaction";import{INITIAL_EVENTS,createEventId}from"./event-utils";export default function DemoApp(){let[weekendsVisible,setWeekendsVisible]=useState(!0),[currentEvents,setCurrentEvents]=useState([]);function handleWeekendsToggle(){setWeekendsVisible(!weekendsVisible)}function handleDateSelect(selectInfo){let title=prompt(`Please enter a new title for your event`),calendarApi=selectInfo.view.calendar;calendarApi.unselect(),title&&calendarApi.addEvent({id:createEventId(),title,start:selectInfo.startStr,end:selectInfo.endStr,allDay:selectInfo.allDay})}function handleEventClick(clickInfo){confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)&&clickInfo.event.remove()}function handleEvents(events){setCurrentEvents(events)}return<div className="demo-app">
      <Sidebar weekendsVisible={weekendsVisible} handleWeekendsToggle={handleWeekendsToggle} currentEvents={currentEvents}/>
      <div className="demo-app-main">


... [truncated 1063 chars] ...

abel>
          <input type="checkbox" checked={weekendsVisible} onChange={handleWeekendsToggle}></input>
          toggle weekends
        </label>
      </div>
      <div className="demo-app-sidebar-section">
        <h2>All Events ({currentEvents.length})</h2>
        <ul>
          {currentEvents.map(event=><SidebarEvent key={event.id} event={event}/>)}
        </ul>
      </div>
    </div>}function SidebarEvent({event}){return<li key={event.id}>
      <b>{formatDate(event.start,{year:`numeric`,month:`short`,day:`numeric`})}</b>
      <i>{event.title}</i>
    </li>}
```

## Apply Minification Excerpt

```jsx
import React,{useState}from"react";import{formatDate}from"@fullcalendar/core";import FullCalendar from"@fullcalendar/react";import dayGridPlugin from"@fullcalendar/daygrid";import timeGridPlugin from"@fullcalendar/timegrid";import interactionPlugin from"@fullcalendar/interaction";import{INITIAL_EVENTS,createEventId}from"./event-utils";export default function DemoApp(){let[weekendsVisible,setWeekendsVisible]=useState(!0),[currentEvents,setCurrentEvents]=useState([]);function handleWeekendsToggle(){setWeekendsVisible(!weekendsVisible)}function handleDateSelect(selectInfo){let title=prompt(`Please enter a new title for your event`),calendarApi=selectInfo.view.calendar;calendarApi.unselect(),title&&calendarApi.addEvent({id:createEventId(),title,start:selectInfo.startStr,end:selectInfo.endStr,allDay:selectInfo.allDay})}function handleEventClick(clickInfo){confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)&&clickInfo.event.remove()}function handleEvents(events){setCurrentEvents(events)}return<div className="demo-app">
      <Sidebar weekendsVisible={weekendsVisible} handleWeekendsToggle={handleWeekendsToggle} currentEvents={currentEvents}/>
      <div className="demo-app-main">


... [truncated 1063 chars] ...

abel>
          <input type="checkbox" checked={weekendsVisible} onChange={handleWeekendsToggle}></input>
          toggle weekends
        </label>
      </div>
      <div className="demo-app-sidebar-section">
        <h2>All Events ({currentEvents.length})</h2>
        <ul>
          {currentEvents.map(event=><SidebarEvent key={event.id} event={event}/>)}
        </ul>
      </div>
    </div>}function SidebarEvent({event}){return<li key={event.id}>
      <b>{formatDate(event.start,{year:`numeric`,month:`short`,day:`numeric`})}</b>
      <i>{event.title}</i>
    </li>}
```

## Sync Minify Excerpt

```jsx
import React,{useState}from"react";import{formatDate}from"@fullcalendar/core";import FullCalendar from"@fullcalendar/react";import dayGridPlugin from"@fullcalendar/daygrid";import timeGridPlugin from"@fullcalendar/timegrid";import interactionPlugin from"@fullcalendar/interaction";import{INITIAL_EVENTS,createEventId}from"./event-utils";export default function DemoApp(){let[weekendsVisible,setWeekendsVisible]=useState(!0),[currentEvents,setCurrentEvents]=useState([]);function handleWeekendsToggle(){setWeekendsVisible(!weekendsVisible)}function handleDateSelect(selectInfo){let title=prompt(`Please enter a new title for your event`),calendarApi=selectInfo.view.calendar;calendarApi.unselect(),title&&calendarApi.addEvent({id:createEventId(),title,start:selectInfo.startStr,end:selectInfo.endStr,allDay:selectInfo.allDay})}function handleEventClick(clickInfo){confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)&&clickInfo.event.remove()}function handleEvents(events){setCurrentEvents(events)}return<div className="demo-app">
      <Sidebar weekendsVisible={weekendsVisible} handleWeekendsToggle={handleWeekendsToggle} currentEvents={currentEvents}/>
      <div className="demo-app-main">


... [truncated 1063 chars] ...

abel>
          <input type="checkbox" checked={weekendsVisible} onChange={handleWeekendsToggle}></input>
          toggle weekends
        </label>
      </div>
      <div className="demo-app-sidebar-section">
        <h2>All Events ({currentEvents.length})</h2>
        <ul>
          {currentEvents.map(event=><SidebarEvent key={event.id} event={event}/>)}
        </ul>
      </div>
    </div>}function SidebarEvent({event}){return<li key={event.id}>
      <b>{formatDate(event.start,{year:`numeric`,month:`short`,day:`numeric`})}</b>
      <i>{event.title}</i>
    </li>}
```

## Async Minify Excerpt

```jsx
import React,{useState}from"react";import{formatDate}from"@fullcalendar/core";import FullCalendar from"@fullcalendar/react";import dayGridPlugin from"@fullcalendar/daygrid";import timeGridPlugin from"@fullcalendar/timegrid";import interactionPlugin from"@fullcalendar/interaction";import{INITIAL_EVENTS,createEventId}from"./event-utils";export default function DemoApp(){let[weekendsVisible,setWeekendsVisible]=useState(!0),[currentEvents,setCurrentEvents]=useState([]);function handleWeekendsToggle(){setWeekendsVisible(!weekendsVisible)}function handleDateSelect(selectInfo){let title=prompt(`Please enter a new title for your event`),calendarApi=selectInfo.view.calendar;calendarApi.unselect(),title&&calendarApi.addEvent({id:createEventId(),title,start:selectInfo.startStr,end:selectInfo.endStr,allDay:selectInfo.allDay})}function handleEventClick(clickInfo){confirm(`Are you sure you want to delete the event '${clickInfo.event.title}'`)&&clickInfo.event.remove()}function handleEvents(events){setCurrentEvents(events)}return<div className="demo-app">
      <Sidebar weekendsVisible={weekendsVisible} handleWeekendsToggle={handleWeekendsToggle} currentEvents={currentEvents}/>
      <div className="demo-app-main">


... [truncated 1063 chars] ...

abel>
          <input type="checkbox" checked={weekendsVisible} onChange={handleWeekendsToggle}></input>
          toggle weekends
        </label>
      </div>
      <div className="demo-app-sidebar-section">
        <h2>All Events ({currentEvents.length})</h2>
        <ul>
          {currentEvents.map(event=><SidebarEvent key={event.id} event={event}/>)}
        </ul>
      </div>
    </div>}function SidebarEvent({event}){return<li key={event.id}>
      <b>{formatDate(event.start,{year:`numeric`,month:`short`,day:`numeric`})}</b>
      <i>{event.title}</i>
    </li>}
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
