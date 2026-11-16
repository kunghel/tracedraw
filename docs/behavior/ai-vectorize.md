# Vectorize with AI

Image > Vectorize with AI (also on a bitmap's right-click menu) sends the
one selected bitmap to a vectorization service the user sets up with
their own key, and places the curves it sends back over the bitmap
(#2713). TraceDraw has no AI of its own; nothing is sent until the user
runs the command, and only the selected picture is sent.

## Setting it up

Tools > Options > AI > Service, section Vectorization:

- Address: where the picture is sent. Empty: the usual address
  (`https://vectorizer.ai/api/v1/vectorize`); any service that takes the
  same kind of upload works, a local one included.
- API id: for services that ask for an id with the key. When it is
  filled, the request carries HTTP basic authentication (`id:key`);
  empty, the key goes alone as a bearer token.
- API key: kept for the session only, unless Remember the key is ticked
  (then saved with the settings, as the AI service's key is).
- Test mode: asks for the service's free test results (`mode=test`),
  where it offers them; they may carry a watermark.
- Replace the bitmap: the curves take the bitmap's place instead of
  going over it.

The command is greyed while AI features are off (Options > AI > General)
or no key is typed; its hover tip says which.

## The request and the answer

A `multipart/form-data` POST to the address with the picture as the
`image` field (PNG, the bitmap's own pixels), and `mode=test` first when
Test mode is ticked. The answer is read as SVG (an `<svg>` element found
in the reply). An error answer is shown with its status and the
service's own words, as for the AI service (`ai.rs`); a busy service
(429, 503) is asked again a few times, as the other AI calls are
(`ai_retry.rs`).

## Placing the curves

The SVG's canvas (its width and height, or the drawing's bounds when it
gives none) is fitted onto the bitmap's box, then turned and moved with
the bitmap's own transform, so the curves lie exactly over the picture.
They come as one group named "AI vectors" on the bitmap's layer, on
top, selected. Placing them, and with Replace the bitmap's removal, is
one undo step ("Vectorize with AI"). If the bitmap was deleted while
the answer was on its way, nothing is placed and the status says so.

Tests: `ai_vectorize::tests`.
