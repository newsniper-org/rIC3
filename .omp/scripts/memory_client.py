import sys
import json
import urllib.request
import urllib.parse
import os
from typing import Any, Optional

DAEMON_URL = "http://localhost:8000"

def make_request(endpoint: str, data: Optional[dict[str, Any]] = None, method: str = "POST") -> str:
    url = f"{DAEMON_URL}/{endpoint}"
    headers = {
        "Content-Type": "application/json;charset=UTF-8",
        "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko)"
    }
    if data is not None:
        req = urllib.request.Request(
            url,
            data=json.dumps(data).encode('utf-8'),
            headers=headers,
            method=method
        )
    else:
        req = urllib.request.Request(
            url,
            headers=headers,
            method=method
        )
    output: str
    try:
        with urllib.request.urlopen(req) as response:
            result = response.read().decode('utf-8')
    except Exception as e:
        output = json.dumps({"status": "error", "message": f"데몬 서버 통신 오류: {e}"}, ensure_ascii=False)
    else:
        output = result
    finally:
        return output

def _save() -> int:
    if len(sys.argv) != 6:
        print("사용법: python3 memory_client.py save <session_id> <role> <content> <tags>")
        return 1
    payload = {
        "session_id": sys.argv[2],
        "role": sys.argv[3],
        "content": sys.argv[4],
        "tags": sys.argv[5]
    }
    print(make_request("save", payload, "POST"))
    return 0

def _search() -> int:
    if len(sys.argv) < 3:
        print("사용법: python3 memory_client.py search <query> [limit]")
        return 1
    payload = {
        "query": sys.argv[2],
        "limit": int(sys.argv[3]) if len(sys.argv) > 3 else 5
    }
    raw_response = make_request("search", payload, "POST")
    try:
        parsed = json.loads(raw_response)
        if "results" in parsed:
            print(json.dumps(parsed["results"], ensure_ascii=False, indent=2))
        else:
            print(json.dumps(parsed, ensure_ascii=False, indent=2))
    except json.JSONDecodeError:
        print(raw_response)
    finally:
        return 0

def _export() -> int:
    if len(sys.argv) < 3:
        print("사용법: python3 memory_client.py export <filepath>")
        return 1
    filepath = sys.argv[2]
    raw_response = make_request("export", None, "GET")
    try:
        parsed = json.loads(raw_response)
        if parsed.get("status") == "success":
            with open(filepath, "w", encoding="utf-8") as f:
                json.dump(parsed.get("records", []), f, ensure_ascii=False, indent=2)
            print(f"[{filepath}] 파일로 데이터를 성공적으로 추출했습니다.")
        else:
            print(f"추출 실패: {raw_response}")
    except Exception as e:
        print(f"데이터 추출 중 오류 발생: {e}\n응답 내용: {raw_response}")
    finally:
        return 0

def _import() -> int:
    if len(sys.argv) < 3:
        print("사용법: python3 memory_client.py import <filepath>")
        return 1
    filepath = sys.argv[2]
    if not os.path.exists(filepath):
        print(f"오류: [{filepath}] 파일을 찾을 수 없습니다.")
        return 1

    try:
        with open(filepath, "r", encoding="utf-8") as f:
            records = json.load(f)

        payload = {"records": records}
        print(f"[{filepath}] 파일에서 {len(records)}개의 레코드를 수입하여 임베딩을 진행합니다. (다소 시간이 소요될 수 있습니다)")
        response = make_request("import", payload, "POST")
        print(response)
    except Exception as e:
        print(f"데이터 수입 중 오류 발생: {e}")
    finally:
        return 0

def main() -> int:
    if len(sys.argv) < 2:
        print("사용법: python3 memory_client.py [save|search|export|import] [arguments]")
        return 1

    command = sys.argv[1]
    exitcode: int
    match command:
        case "save":
            exitcode = _save()
        case "search":
            exitcode = _search()
        case "export":
            exitcode = _export()
        case "import":
            exitcode = _import()
        case _:
            print(f"알 수 없는 명령어: {command}")
            exitcode = 1

    return exitcode

if __name__ == "__main__":
    ec: int = main()
    sys.exit(ec)
