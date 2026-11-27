# Undo boundaries under rapid UI updates

## Goal

Investigate the following scenario: A drag or text edit produces too many undo steps or restores an unexpected selection.

## Questions

- What determines undo boundaries during rapid Excalidraw drag or text updates?
- Why might one interaction create too many history entries or restore the wrong selection?
- Which scene and UI state should survive undo, and which state is transient?
