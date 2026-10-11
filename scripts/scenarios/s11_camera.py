"""Camera video: a CloakBrowser profile hands a page a video file in place of the camera.

Needs ffmpeg, both to make the test clip and for the app to convert it.
"""
from scn import *
import shutil, tempfile
if not shutil.which("ffmpeg"):
    print("SKIP ffmpeg is not installed"); sys.exit(0)
wait_api()
work = tempfile.mkdtemp(prefix="scn-camera-")
clip = os.path.join(work, "clip.mp4")
subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=30", "-t", "3", "-pix_fmt", "yuv420p", clip], check=True)
created = []
try:
    status, r = api("POST", "/api/v2/automation/profiles", {"title": "S11 camera", "browser": "cloakbrowser", "fingerprint": {"os": "win"}, "camera_video": clip})
    check("a profile is created with a camera video", status == 200 and r.get("success"), f"{status} {str(r)[:160]}")
    uuid = r["data"]["uuid"]; created.append(uuid)
    status, r = api("POST", "/api/v2/automation/profiles", {"title": "S11 no such file", "browser": "cloakbrowser", "camera_video": os.path.join(work, "missing.mp4")})
    check("a video file that does not exist is refused", status == 400 and "CAMERA_VIDEO_NOT_FOUND" in str(r), f"{status} {str(r)[:120]}")
    if status == 200: created.append(r["data"]["uuid"])

    status, st = start(uuid)
    check("the profile starts", status == 200, f"{status} {str(st)[:160]}")
    time.sleep(4)
    out = subprocess.run(["node", "camera-probe.mjs", str(st["debug_port"])], cwd=HERE, capture_output=True, text=True, timeout=90)
    try:
        seen = json.loads(out.stdout)
    except ValueError:
        seen = {"error": (out.stdout + out.stderr)[-300:]}
    print("    page sees:", json.dumps(seen)[:400])
    check("the page gets a camera stream", "error" not in seen, seen.get("error", ""))
    check("the stream is the video: 1280x720 and not black", (seen.get("width"), seen.get("height")) == (1280, 720) and seen.get("brightness", 0) > 10, f"{seen.get('width')}x{seen.get('height')}, brightness {seen.get('brightness')}")
    check("the picture moves", seen.get("moving") is True)
    check("the camera's name does not give away a path", "/" not in seen.get("label", "/"), seen.get("label"))
    fake = [d for d in seen.get("devices", []) if "Fake" in d]
    if fake: note(f"known limit: the microphone is listed as {fake[0]!r}, and the camera is named after its file ({seen.get('label')!r})")
    stop(uuid)
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    shutil.rmtree(work, ignore_errors=True)
summary()
