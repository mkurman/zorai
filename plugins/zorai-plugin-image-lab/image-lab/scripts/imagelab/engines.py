import shutil
from pathlib import Path

from .cloud import generate_image
from .paths import models_dir

CLOUD_SIZES = {"1:1": "1024x1024", "3:2": "1536x1024", "16:9": "1536x1024", "2:3": "1024x1536", "9:16": "1024x1536"}
LOCAL_SIZES = {"1:1": (768, 768), "3:2": (912, 608), "16:9": (1024, 576), "2:3": (608, 912), "9:16": (576, 1024)}
DEFAULT_LOCAL_MODEL = "stabilityai/sdxl-turbo"


def local_ready(model: str = DEFAULT_LOCAL_MODEL) -> bool:
    try:
        import diffusers  # noqa: F401
        import torch  # noqa: F401
    except ImportError:
        return False
    return (models_dir() / model.replace("/", "--") / "model_index.json").is_file()


class CloudEngine:
    name = "cloud"

    def __init__(self, thread: str | None = None, route: dict | None = None):
        self.thread = thread
        self.route = route or {}

    def identity(self, settings: dict) -> str:
        image = self.route if self.route.get("provider") else settings.get("image") or {}
        return f"cloud:{image.get('provider', '')}:{image.get('model', '')}"

    def generate(self, prompt: str, aspect: str, seed: int, target: Path) -> dict:
        result = generate_image(prompt, CLOUD_SIZES.get(aspect, "1024x1024"), self.thread, self.route)
        self.thread = result.get("thread_id") or self.thread
        shutil.copyfile(result["path"], target)
        return {"thread_id": self.thread, "revised_prompt": result.get("revised_prompt"), "seed": None}

    def inpaint(self, *args, **kwargs):
        raise RuntimeError("The cloud engine cannot repaint one region; Image Lab regenerates the whole image with the fix instead.")


class LocalEngine:
    name = "local"

    def __init__(self, model: str = DEFAULT_LOCAL_MODEL, device: str = "auto"):
        import torch
        from diffusers import AutoPipelineForText2Image

        if not local_ready(model):
            raise RuntimeError("The local image model is not installed. Open Image Lab, Setup, and press Install local engine.")
        self.model = model
        self.device = "cuda" if device in ("auto", "cuda") and torch.cuda.is_available() else "cpu"
        dtype = torch.float16 if self.device == "cuda" else torch.float32
        self._torch = torch
        folder = models_dir() / model.replace("/", "--")
        variant = "fp16" if any(folder.rglob("*.fp16.safetensors")) else None
        self.pipe = AutoPipelineForText2Image.from_pretrained(str(folder), torch_dtype=dtype, variant=variant).to(self.device)
        self._inpaint = None

    def identity(self, settings: dict) -> str:
        return f"local:{self.model}"

    def generate(self, prompt: str, aspect: str, seed: int, target: Path) -> dict:
        width, height = LOCAL_SIZES.get(aspect, (768, 768))
        generator = self._torch.Generator(device=self.device).manual_seed(seed)
        image = self.pipe(prompt=prompt[:600], width=width, height=height, num_inference_steps=4, guidance_scale=0.0, generator=generator).images[0]
        image.save(target)
        return {"seed": seed}

    def inpaint(self, source: Path, box: list, prompt: str, seed: int, target: Path) -> dict:
        from diffusers import AutoPipelineForInpainting
        from PIL import Image, ImageDraw, ImageFilter

        if self._inpaint is None:
            self._inpaint = AutoPipelineForInpainting.from_pipe(self.pipe)
        image = Image.open(source).convert("RGB")
        mask = Image.new("L", image.size, 0)
        x0, y0, x1, y1 = box
        ImageDraw.Draw(mask).rectangle((x0 * image.width, y0 * image.height, x1 * image.width, y1 * image.height), fill=255)
        mask = mask.filter(ImageFilter.GaussianBlur(max(2, image.width // 100)))
        generator = self._torch.Generator(device=self.device).manual_seed(seed)
        result = self._inpaint(prompt=prompt[:600], image=image, mask_image=mask, width=image.width // 8 * 8, height=image.height // 8 * 8, num_inference_steps=4, strength=0.9, guidance_scale=0.0, generator=generator).images[0]
        result.resize(image.size).save(target)
        return {"seed": seed}


def make_engine(kind: str, settings: dict, thread: str | None = None, route: dict | None = None):
    if kind == "local":
        return LocalEngine(settings.get("local_model") or DEFAULT_LOCAL_MODEL, settings.get("device") or "auto")
    return CloudEngine(thread, route)
