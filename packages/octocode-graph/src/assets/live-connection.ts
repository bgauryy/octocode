// Decoding gzip is asynchronous. After a failed or closed connection, only the
// next socket may advance the viewer's last applied sequence.
export function orderedInbox<D>(
  decode: (data: D) => string | Promise<string>,
  receive: (text: string) => void,
  failed: (error: unknown) => void
) {
  let tail = Promise.resolve(),
    active = true;
  return {
    push(data: D) {
      tail = tail
        .then(async () => {
          if (!active) return;
          const text = await decode(data);
          if (active) receive(text);
        })
        .catch(error => {
          if (active) {
            active = false;
            failed(error);
          }
        });
      return tail;
    },
    close() {
      active = false;
    },
  };
}
