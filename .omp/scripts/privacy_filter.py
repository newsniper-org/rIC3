import sys
import re
import json

# workspace.yml의 패턴과 동일한 보안 검증 정규표현식
SENSITIVE_PATTERNS = [
    r"(?i)-----BEGIN[ A-Z]*PRIVATE KEY-----.*?-----END[ A-Z]*PRIVATE KEY-----",
    r"(?i)(api[_-]?key|secret|token|password)[\"'\s:=]+([A-Za-z0-9_\-]{16,})",
    r"sk-[a-zA-Z0-9]{32,}",
    r"(?i)ghp_[a-zA-Z0-9]{36}" # GitHub Personal Access Token
]

def scan_and_mask(text: str) -> str:
    """
    텍스트 내의 민감 정보를 감지하고 [REDACTED_BY_PRIVACY_FILTER]로 마스킹합니다.
    """
    masked_text = text
    for pattern in SENSITIVE_PATTERNS:
        # 그룹 캡처가 있는 경우 값 부분만 마스킹하기 위한 로직
        if "()" in pattern or "(?i)(" in pattern:
            # 복잡한 교체를 위해 정규식 컴파일
            regex = re.compile(pattern, re.DOTALL)
            
            def replace_match(match):
                if len(match.groups()) > 1:
                    # key=value 형태에서 value(그룹 2)만 마스킹
                    full_match = match.group(0)
                    secret_val = match.group(2)
                    return full_match.replace(secret_val, "[REDACTED_BY_PRIVACY_FILTER]")
                return "[REDACTED_BY_PRIVACY_FILTER]"
                
            masked_text = regex.sub(replace_match, masked_text)
        else:
            masked_text = re.sub(pattern, "[REDACTED_BY_PRIVACY_FILTER]", masked_text, flags=re.IGNORECASE | re.DOTALL)
            
    return masked_text

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("사용법: python3.14 privacy_filter.py <텍스트_문자열>")
        sys.exit(1)
        
    input_text = sys.argv[1]
    result = scan_and_mask(input_text)
    
    output = {
        "original_length": len(input_text),
        "masked_text": result,
        "is_modified": input_text != result
    }
    
    print(json.dumps(output, ensure_ascii=False, indent=2))
