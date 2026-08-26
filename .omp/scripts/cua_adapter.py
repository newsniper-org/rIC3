import sys
import subprocess
import json
import base64
import os
from typing import Dict, Any

def load_dotenv(filepath: str = ".env") -> None:
    """
    프로젝트 루트 또는 스크립트 실행 위치의 .env 파일을 읽어 환경 변수로 등록합니다.
    외부 라이브러리(python-dotenv) 의존성 없이 Python 내장 기능만을 사용하여
    컴파일 및 런타임 오류 가능성을 최소화하도록 설계되었습니다.
    """
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__))) if '__file__' in globals() else '.'
    target_path = os.path.join(base_dir, filepath)

    if not os.path.exists(target_path):
        target_path = filepath

    if os.path.exists(target_path):
        try:
            with open(target_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    if "=" in line:
                        key, value = line.split("=", 1)
                        key = key.strip()
                        value = value.strip().strip("'\"")
                        os.environ[key] = value
        except Exception as e:
            sys.stderr.write(f".env 파일을 읽는 중 오류가 발생했습니다: {e}\n")

# 스크립트 실행 시 가장 먼저 .env 파일을 로드합니다.
load_dotenv()

ENABLE_OCR_CUA = os.environ.get("ENABLE_OCR_CUA", "false").lower() in ("true", "1", "yes", "on")

try:
    import pyautogui
except ImportError:
    pyautogui = None

try:
    if ENABLE_OCR_CUA:
        import pytesseract
        from PIL import Image
    else:
        pytesseract = None
        Image = None
except ImportError:
    pytesseract = None
    Image = None


def execute_bash(command: str) -> Dict[str, Any]:
    try:
        result = subprocess.run(command, shell=True, check=False, capture_output=True, text=True)
        return {
            "status": "success" if result.returncode == 0 else "error",
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.returncode
        }
    except Exception as e:
        return {"status": "exception", "error": str(e)}


def screen_capture() -> Dict[str, Any]:
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__))) if '__file__' in globals() else '.'
    temp_file = os.path.join(base_dir, "tmp", "capture.png")
    os.makedirs(os.path.dirname(temp_file), exist_ok=True)

    try:
        # Wayland 환경에서는 pyautogui가 설치되어 있더라도 캡처가 실패할 확률이 높으므로 강제로 분기합니다.
        if sys.platform == "linux" and os.environ.get("WAYLAND_DISPLAY"):
            try:
                subprocess.run(["grimshot", "save", "screen", temp_file], check=True)
            except FileNotFoundError:
                subprocess.run(["grim", temp_file], check=True)
        else:
            if pyautogui is not None:
                screenshot = pyautogui.screenshot()
                screenshot.save(temp_file)
            else:
                if sys.platform == "darwin":
                    subprocess.run(["screencapture", "-x", temp_file], check=True)
                else:
                    subprocess.run(["scrot", temp_file], check=True)

        with open(temp_file, "rb") as image_file:
            encoded_string = base64.b64encode(image_file.read()).decode('utf-8')

        return {
            "status": "success",
            "format": "png",
            "data": encoded_string
        }
    except Exception as e:
        return {"status": "exception", "error": str(e)}


def simulate_click(x: int, y: int) -> None:
    """
    OS 및 디스플레이 서버 환경(Wayland/X11)을 감지하여 가장 안전한 방법으로 마우스 클릭을 시뮬레이션합니다.
    """
    if sys.platform == "linux" and os.environ.get("WAYLAND_DISPLAY"):
        # Wayland 환경: uinput 인터페이스를 활용하는 ydotool을 통한 우회 시도
        subprocess.run(["ydotool", "mousemove", "-a", str(x), str(y)], check=True)
        # ydotool 버전 및 설정에 따라 좌클릭 식별자가 다를 수 있으나 보편적으로 '1' 또는 '0xC0'을 사용합니다.
        subprocess.run(["ydotool", "click", "1"], check=True)
    else:
        # X11 및 기타 환경
        if pyautogui is not None:
            pyautogui.click(x, y)
        else:
            if sys.platform == "darwin":
                subprocess.run(["cliclick", f"c:{x},{y}"], check=True)
            else:
                subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", "1"], check=True)


def find_and_click_ui(text_to_find: str) -> Dict[str, Any]:
    """
    화면 캡처 후 지정된 텍스트가 포함된 UI 요소를 찾아 클릭합니다.
    Wayland 지원을 위해 pyautogui의 자체 캡처 대신 screen_capture()의 결과를 재사용하도록 분리 설계되었습니다.
    """
    if not ENABLE_OCR_CUA:
        return {
            "status": "error",
            "message": "OCR 기반 UI 제어 기능이 현재 환경 변수(.env 등)에 의해 비활성화되어 있습니다."
        }

    if pytesseract is None or Image is None:
        return {
            "status": "error",
            "message": "기능이 활성화되었으나 필수 라이브러리(pytesseract, Pillow)가 설치되지 않아 모듈을 실행할 수 없습니다."
        }

    try:
        # 1. 환경에 맞는 화면 캡처 수행 (Wayland 우회 포함)
        capture_result = screen_capture()
        if capture_result.get("status") != "success":
            return {"status": "error", "message": f"화면 캡처 실패: {capture_result.get('error', '알 수 없는 오류')}"}

        # 2. 저장된 임시 캡처 이미지 로드
        base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__))) if '__file__' in globals() else '.'
        temp_file = os.path.join(base_dir, "tmp", "capture.png")

        with Image.open(temp_file) as img:
            # 3. 이미지 내 텍스트 바운딩 박스 추출
            ocr_data = pytesseract.image_to_data(img, output_type=pytesseract.Output.DICT)

            found = False
            target_x, target_y = 0, 0

            for i in range(len(ocr_data['text'])):
                word = ocr_data['text'][i].strip()
                if not word:
                    continue

                if text_to_find.lower() in word.lower():
                    x = ocr_data['left'][i]
                    y = ocr_data['top'][i]
                    w = ocr_data['width'][i]
                    h = ocr_data['height'][i]

                    target_x = x + (w // 2)
                    target_y = y + (h // 2)
                    found = True
                    break

        # 4. 검출 결과에 따른 OS별 마우스 클릭 분기 실행
        if found:
            simulate_click(target_x, target_y)
            return {
                "status": "success",
                "message": f"'{text_to_find}' 텍스트를 인식하여 중앙 좌표({target_x}, {target_y})를 클릭했습니다."
            }
        else:
            return {
                "status": "error",
                "message": f"화면에서 '{text_to_find}' 텍스트를 식별하지 못했습니다."
            }

    except Exception as e:
        return {"status": "exception", "error": str(e)}


def route_cua_action(action: str, parameters: Dict[str, Any]) -> str:
    if action == "bash":
        return json.dumps(execute_bash(parameters.get("command", "")), ensure_ascii=False)
    elif action == "screenshot":
        return json.dumps(screen_capture(), ensure_ascii=False)
    elif action == "click_ui":
        text_target = parameters.get("text", "")
        if not text_target:
            return json.dumps({"status": "error", "message": "클릭할 대상 텍스트(text)가 파라미터로 제공되지 않았습니다."}, ensure_ascii=False)
        return json.dumps(find_and_click_ui(text_target), ensure_ascii=False)
    else:
        return json.dumps({"status": "error", "message": f"지원하지 않는 CUA 액션입니다: {action}"}, ensure_ascii=False)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        if len(sys.argv) == 2 and sys.argv[1] == "test":
            print(json.dumps({"status": "success", "message": "CUA 어댑터 모듈이 정상적으로 로드되었습니다."}, ensure_ascii=False))
            sys.exit(0)
        print("사용법: python3.14 cua_adapter.py <action> <parameters_json>")
        print("지원 액션: bash, screenshot, click_ui")
        sys.exit(1)

    action_type = sys.argv[1]

    try:
        params = json.loads(sys.argv[2])
    except json.JSONDecodeError:
        print(json.dumps({"status": "error", "message": "파라미터가 유효한 JSON 포맷이 아닙니다."}, ensure_ascii=False))
        sys.exit(1)

    print(route_cua_action(action_type, params))
