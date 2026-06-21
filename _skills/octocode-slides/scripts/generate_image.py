#!/usr/bin/env python3

# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "google-genai",
#   "pillow",
# ]
# ///

"""
Generate and edit images using Nano Banana 2 (Gemini 3.1 Flash Image Preview).

Usage examples:
    uv run skills/octocode-slides/scripts/generate_image.py \
      --prompt "A serene Japanese garden at sunrise" \
      --filename "assets/hero-2026-05-10-22-00-00.png" \
      --resolution 2K \
      --aspect-ratio 16:9

    uv run skills/octocode-slides/scripts/generate_image.py \
      --prompt "Turn this into a cinematic poster" \
      --filename "assets/edited.png" \
      --input-image "assets/reference.png" \
      --resolution 4K

API key resolution order:
  1. --api-key argument
  2. GEMINI_API_KEY environment variable

Get your key: https://aistudio.google.com/apikey
"""

import argparse
import os
import sys
from io import BytesIO
from pathlib import Path

SUPPORTED_ASPECT_RATIOS = [
    "1:1", "1:4", "1:8", "2:3", "3:2", "3:4", "4:1",
    "4:3", "4:5", "5:4", "8:1", "9:16", "16:9", "21:9",
]
SUPPORTED_RESOLUTIONS = ["512px", "1K", "2K", "4K"]
MAX_REFERENCE_IMAGES = 14


def get_api_key(provided_key: str | None) -> str | None:
    """Get API key from argument first, then environment."""
    if provided_key:
        return provided_key
    return os.environ.get("GEMINI_API_KEY")


def save_png(image, output_path: Path) -> None:
    """Save image as PNG in RGB mode."""
    if image.mode == "RGBA":
        image.convert("RGB").save(str(output_path), "PNG")
    elif image.mode == "RGB":
        image.save(str(output_path), "PNG")
    else:
        image.convert("RGB").save(str(output_path), "PNG")


def auto_detect_resolution(input_images, user_resolution: str) -> str:
    """Auto-select output size from input references when user left default 1K."""
    if user_resolution != "1K" or not input_images:
        return user_resolution

    max_dim = max(max(image.size) for image in input_images)
    if max_dim >= 3000:
        return "4K"
    if max_dim >= 1500:
        return "2K"
    if max_dim <= 640:
        return "512px"
    return "1K"


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate/edit images with Nano Banana 2 (gemini-3.1-flash-image-preview)"
    )
    parser.add_argument("--prompt", "-p", required=True, help="Image prompt/instructions")
    parser.add_argument("--filename", "-f", required=True, help="Output filename (.png recommended)")
    parser.add_argument(
        "--input-image",
        "-i",
        action="append",
        default=[],
        help=(
            "Optional reference/input image path. Repeat flag for multiple images "
            f"(up to {MAX_REFERENCE_IMAGES})."
        ),
    )
    parser.add_argument(
        "--resolution",
        "-r",
        choices=SUPPORTED_RESOLUTIONS,
        default="1K",
        help="Output resolution: 512px, 1K (default), 2K, or 4K",
    )
    parser.add_argument(
        "--aspect-ratio",
        "-a",
        choices=SUPPORTED_ASPECT_RATIOS,
        help="Optional output aspect ratio",
    )
    parser.add_argument("--api-key", "-k", help="Gemini API key (overrides GEMINI_API_KEY env var)")

    args = parser.parse_args()

    api_key = get_api_key(args.api_key)
    if not api_key:
        print("Error: No API key provided.", file=sys.stderr)
        print("  1. Pass --api-key <your-key>", file=sys.stderr)
        print("  2. Or set: export GEMINI_API_KEY='...'", file=sys.stderr)
        print("  Get a key: https://aistudio.google.com/apikey", file=sys.stderr)
        sys.exit(1)

    if len(args.input_image) > MAX_REFERENCE_IMAGES:
        print(
            f"Error: Too many input images ({len(args.input_image)}). "
            f"Maximum supported: {MAX_REFERENCE_IMAGES}.",
            file=sys.stderr,
        )
        sys.exit(1)

    from google import genai
    from google.genai import types
    from PIL import Image as PILImage

    client = genai.Client(api_key=api_key)

    output_path = Path(args.filename)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    input_images = []
    if args.input_image:
        for image_path in args.input_image:
            try:
                image = PILImage.open(image_path)
                input_images.append(image)
                print(f"Loaded input image: {image_path}")
            except Exception as exc:
                print(f"Error loading input image '{image_path}': {exc}", file=sys.stderr)
                sys.exit(1)

    output_resolution = auto_detect_resolution(input_images, args.resolution)

    image_config_kwargs = {"image_size": output_resolution}
    if args.aspect_ratio:
        image_config_kwargs["aspect_ratio"] = args.aspect_ratio

    if input_images:
        contents = [args.prompt, *input_images]
        print(
            f"Editing/generating from {len(input_images)} reference image(s), "
            f"resolution {output_resolution}"
            + (f", aspect ratio {args.aspect_ratio}" if args.aspect_ratio else "")
            + "..."
        )
    else:
        contents = [args.prompt]
        print(
            f"Generating image, resolution {output_resolution}"
            + (f", aspect ratio {args.aspect_ratio}" if args.aspect_ratio else "")
            + "..."
        )

    try:
        response = client.models.generate_content(
            model="gemini-3.1-flash-image-preview",
            contents=contents,
            config=types.GenerateContentConfig(
                response_modalities=["TEXT", "IMAGE"],
                image_config=types.ImageConfig(**image_config_kwargs),
            ),
        )

        image_parts = 0
        saved_paths = []

        for part in response.parts:
            if part.text is not None:
                print(f"Model response: {part.text}")
            elif part.inline_data is not None:
                image_parts += 1

                image_data = part.inline_data.data
                if isinstance(image_data, str):
                    import base64
                    image_data = base64.b64decode(image_data)

                image = PILImage.open(BytesIO(image_data))

                if image_parts == 1:
                    current_path = output_path
                else:
                    current_path = output_path.with_name(
                        f"{output_path.stem}-{image_parts}{output_path.suffix or '.png'}"
                    )

                save_png(image, current_path)
                saved_paths.append(current_path.resolve())

        if saved_paths:
            print("\nSaved:")
            for path in saved_paths:
                print(f"  {path}")
        else:
            print("Error: No image was generated in the response.", file=sys.stderr)
            sys.exit(1)

    except Exception as exc:
        print(f"Error generating image: {exc}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
