# NVIDIA CUDA 13.2.1 환경이 구성된 SUSE Enterprise 16 (devel) 베이스 이미지 사용
FROM docker.io/nvidia/cuda:13.2.1-devel-suse16

# 1. uv 공식 바이너리를 컨테이너로 직접 복사
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

# 2. C 확장을 위한 필수 시스템 패키지 설치 (Python 설치는 uv에 위임)
RUN zypper refresh && zypper install -y \
    gcc \
    gcc-c++ \
    make \
    sqlite3-devel \
    tar \
    gzip \
    && zypper clean -a

WORKDIR /app

# 3. 의존성 파일 복사 및 uv를 활용한 동기화
# uv가 스스로 적절한 Python 3.14 런타임을 다운로드하고 패키지를 설치합니다.
COPY pyproject.toml .
RUN uv sync --python 3.14

# 4. 서버 스크립트 복사
COPY daemon.py .

# 5. SQLite 데이터베이스가 저장될 볼륨 디렉토리 생성
RUN mkdir -p /app/data

# 6. FastAPI 포트 노출
EXPOSE 8000

# 7. uv run을 사용하여 고립된 3.14 환경에서 Uvicorn 실행
CMD ["uv", "run", "--python", "3.14", "uvicorn", "daemon:app", "--host", "0.0.0.0", "--port", "8000"]
