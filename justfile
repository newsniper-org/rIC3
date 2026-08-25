# justfile for oh-my-pi workspace management
default:
    @just --list

# 1. 통합 워크스페이스 구조 및 IPC 통신 디렉터리 초기화
init-workspace:
    mkdir -p .omp/data
    mkdir -p .local-requests-from
    mkdir -p .local-requests-to
    mkdir -p .local-replies-from
    mkdir -p .local-replies-to
    mkdir -p .local-mentions-by
    # 루트 디렉터리의 Python 3.14 환경 의존성 동기화
    uv sync --python 3.14
    podman build --no-cache -t omp-memory-daemon -f .omp/server/Dockerfile .omp/server/
    @echo "데이터베이스 준비, IPC 디렉터리 세팅 및 uv 환경 구축이 완료되었습니다."

# 2. 데몬 컨테이너 실행 (NVIDIA GPU 리소스 패스스루 포함)
run-daemon:
    podman run -d --name memory-daemon \
        --network host \
        --device nvidia.com/gpu=all \
        -p 8000:8000 \
        -v $(pwd)/.omp/data:/app/data \
        omp-memory-daemon

# 3. 데몬 컨테이너 중지 및 삭제 (오류 방지를 위해 실패 시 무시)
stop-daemon:
    podman stop memory-daemon || true
    podman rm memory-daemon || true

# 4. 기본 모델로 에이전트 시작
start OMP_CONFIG_FILE=".omp/workspace.yml":
    omp start --config "{{OMP_CONFIG_FILE}}"

# 5. 민감 정보 스캐너 구동 테스트
scan-privacy TEXT:
    uv run --python 3.14 .omp/scripts/privacy_filter.py "{{TEXT}}"

# 6. 특정 폴백 모델로 수동 전환하여 시작
switch-model MODEL:
    omp start --config .omp/workspace.yml --model {{MODEL}}

# 7. 현재 세션 데이터를 메모리 DB 데몬에 저장
save-memory SESSION_ID ROLE CONTENT TAGS:
    uv run --python 3.14 .omp/scripts/memory_client.py save {{SESSION_ID}} {{ROLE}} "{{CONTENT}}" "{{TAGS}}"

# 8. 메모리 DB 데몬 검색 테스트
search-memory QUERY LIMIT="5":
    uv run --python 3.14 .omp/scripts/memory_client.py search "{{QUERY}}" {{LIMIT}}

# 9. 컨텍스트 데이터베이스 JSON 파일로 추출 (Logical Backup)
export-db FILEPATH="context_backup.json":
    uv run --python 3.14 .omp/scripts/memory_client.py export {{FILEPATH}}

# 10. JSON 파일로부터 컨텍스트 데이터베이스 적재 (Vector Re-embedding)
import-db FILEPATH="context_backup.json":
    uv run --python 3.14 .omp/scripts/memory_client.py import {{FILEPATH}}

# 11. CUA(컴퓨터 사용) 어댑터 독립 테스트 구동
test-cua:
    uv run --python 3.14 .omp/scripts/cua_adapter.py test

# 12. 다른 프로젝트로 메시지 발송 (IPC 파일 생성)
send-message RECEIVER MESSAGE:
    mkdir -p .local-requests-to/{{RECEIVER}}
    echo "{{MESSAGE}}" > .local-requests-to/{{RECEIVER}}/$(date +%s).md
    @echo "{{RECEIVER}} 프로젝트로 메시지 발송이 예약되었습니다."

# 13. 임시 파일 정리
clean:
    rm -rf .omp/tmp/*

# 14. rIC3 릴리스 빌드
build-ric3:
    cargo build --release

# 15. 벤치마크 하네스 스모크 실행 (fvbench, 코퍼스 불필요)
bench-smoke TIMEOUT="60":
    uv run --python 3.14 bench/harness.py examples/fvbench --timeout {{TIMEOUT}}

# 16. 벤치마크 하네스 (임의 코퍼스 / 엔진 지정)
bench CORPUS TIMEOUT="3600" ENGINE="ic3":
    uv run --python 3.14 bench/harness.py {{CORPUS}} --timeout {{TIMEOUT}} --engine {{ENGINE}}

# 17. 코퍼스 인스턴스 목록만 확인 (실행 없음)
bench-list CORPUS:
    uv run --python 3.14 bench/harness.py {{CORPUS}} --dry-run

# 18. CAV'25 논문 기준선 재현 (840 케이스 / AIGER / DynAMic / 32GB / 3600s)
#     rIC3-ic3 구성: 논문 §7.1 및 아티팩트 README 기준.
#     주의: 현재 rIC3는 --dynamic 과 --drop-po 동시 활성화를 거부하며
#     ("cannot enable both dynamic and drop-po", src/ic3/mod.rs:132)
#     --drop-po 기본값이 true 이므로 명시적으로 꺼야 한다.
bench-baseline OUT="bench/results/baseline":
    uv run --python 3.14 bench/harness.py bench/corpus \
        --instance-list bench/corpus/reference/cases-840.txt \
        --ext aig --dedup-by-stem \
        --engine ic3 --engine-arg=--dynamic --engine-arg=--drop-po=false \
        --timeout 3600 --memory-limit-mb 32768 \
        --out {{OUT}}

# 19. 기준선 측정 결과를 논문 아티팩트와 대조
bench-compare RESULTS:
    uv run --python 3.14 bench/compare.py {{RESULTS}} \
        bench/corpus/reference/rIC3-ic3-cav25.txt
