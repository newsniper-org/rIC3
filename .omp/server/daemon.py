import sqlite3
import os
import struct
import faulthandler
from datetime import datetime
from typing import List, Dict, Any, Optional
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

# C/C++ 네이티브 확장에서 발생하는 치명적 충돌(Segfault 등) 시 파이썬 콜스택을 강제로 출력하도록 활성화합니다.
faulthandler.enable()

try:
    import sqlite_vec
except ImportError:
    raise RuntimeError("sqlite-vec 패키지가 설치되지 않았습니다.")

try:
    from sentence_transformers import SentenceTransformer
except ImportError:
    raise RuntimeError("sentence-transformers 패키지가 설치되지 않았습니다.")

MODEL_NAME = 'paraphrase-multilingual-MiniLM-L12-v2'
_model_instance: Optional[SentenceTransformer] = None

app = FastAPI(title="Oh-My-Pi Context Memory Daemon")
DB_PATH = os.environ.get("DB_PATH", "/app/data/context.db")

class SaveRequest(BaseModel):
    session_id: str
    role: str
    content: str
    tags: str

class SearchRequest(BaseModel):
    query: str
    limit: int = 5

class MemoryRecord(BaseModel):
    session_id: str
    timestamp: str
    role: str
    content: str
    tags: str

class ImportRequest(BaseModel):
    records: List[MemoryRecord]

def get_db_connection() -> sqlite3.Connection:
    conn = sqlite3.connect(DB_PATH)
    conn.enable_load_extension(True)
    sqlite_vec.load(conn)
    conn.enable_load_extension(False)
    conn.row_factory = sqlite3.Row
    return conn

def get_model() -> SentenceTransformer:
    global _model_instance
    if _model_instance is None:
        print(f"[{datetime.now().isoformat()}] 임베딩 모델 로딩을 시작합니다: {MODEL_NAME}")
        _model_instance = SentenceTransformer(MODEL_NAME)
        print(f"[{datetime.now().isoformat()}] 임베딩 모델 로딩 완료.")
    return _model_instance

@app.on_event("startup")
def init_db():
    conn = get_db_connection()
    cursor = conn.cursor()

    cursor.execute('''
        CREATE TABLE IF NOT EXISTS memory (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT,
            timestamp TEXT,
            role TEXT,
            content TEXT,
            tags TEXT
        )
    ''')

    cursor.execute('''
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
            content, tags, content='memory', content_rowid='id'
        )
    ''')

    cursor.execute('''
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(
            embedding float[384]
        )
    ''')

    cursor.execute('''
        CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory BEGIN
            INSERT INTO memory_fts(rowid, content, tags)
            VALUES (new.id, new.content, new.tags);
        END;
    ''')

    conn.commit()
    conn.close()
    print(f"[{datetime.now().isoformat()}] 데이터베이스 스키마 검증 완료.")

@app.post("/save")
def save_memory(req: SaveRequest):
    try:
        model = get_model()
        conn = get_db_connection()
        cursor = conn.cursor()

        cursor.execute('''
            INSERT INTO memory (session_id, timestamp, role, content, tags)
            VALUES (?, ?, ?, ?, ?)
        ''', (req.session_id, datetime.now().isoformat(), req.role, req.content, req.tags))

        row_id = cursor.lastrowid

        text_to_embed = f"{req.tags} {req.content}"
        embedding = model.encode(text_to_embed).tolist()
        embedding_bytes = struct.pack(f"{len(embedding)}f", *embedding)

        cursor.execute('''
            INSERT INTO memory_vec (rowid, embedding)
            VALUES (?, ?)
        ''', (row_id, embedding_bytes))

        conn.commit()
        conn.close()
        return {"status": "success", "id": row_id}
    except Exception as e:
        print(f"Save 오류 발생: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

@app.post("/search")
def search_memory(req: SearchRequest):
    try:
        model = get_model()
        conn = get_db_connection()
        cursor = conn.cursor()
        RRF_K = 60

        cursor.execute('''
            SELECT rowid, rank
            FROM memory_fts
            WHERE memory_fts MATCH ?
            LIMIT 20
        ''', (req.query,))
        fts_results = cursor.fetchall()

        query_embedding = model.encode(req.query).tolist()
        query_bytes = struct.pack(f"{len(query_embedding)}f", *query_embedding)

        cursor.execute('''
            SELECT rowid, distance
            FROM memory_vec
            WHERE embedding MATCH ? AND k = 20
        ''', (query_bytes,))
        vec_results = cursor.fetchall()

        scores: Dict[int, float] = {}
        for rank_idx, row in enumerate(fts_results):
            row_id = row['rowid']
            scores[row_id] = scores.get(row_id, 0.0) + (1.0 / (RRF_K + rank_idx + 1))

        for rank_idx, row in enumerate(vec_results):
            row_id = row['rowid']
            scores[row_id] = scores.get(row_id, 0.0) + (1.0 / (RRF_K + rank_idx + 1))

        ranked_row_ids = sorted(scores.keys(), key=lambda x: scores[x], reverse=True)[:req.limit]

        results = []
        if ranked_row_ids:
            placeholders = ','.join('?' for _ in ranked_row_ids)
            cursor.execute(f'''
                SELECT id, session_id, timestamp, role, content, tags
                FROM memory
                WHERE id IN ({placeholders})
            ''', ranked_row_ids)

            rows_by_id = {row['id']: dict(row) for row in cursor.fetchall()}
            for row_id in ranked_row_ids:
                if row_id in rows_by_id:
                    results.append(rows_by_id[row_id])

        conn.close()
        return {"results": results}
    except Exception as e:
        print(f"Search 오류 발생: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

@app.get("/export")
def export_memory():
    try:
        conn = get_db_connection()
        cursor = conn.cursor()
        cursor.execute("SELECT session_id, timestamp, role, content, tags FROM memory")
        rows = cursor.fetchall()
        conn.close()
        return {"status": "success", "records": [dict(row) for row in rows]}
    except Exception as e:
        print(f"Export 오류 발생: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

@app.post("/import")
def import_memory(req: ImportRequest):
    try:
        model = get_model()
        conn = get_db_connection()
        cursor = conn.cursor()
        inserted_count = 0

        for record in req.records:
            cursor.execute('''
                INSERT INTO memory (session_id, timestamp, role, content, tags)
                VALUES (?, ?, ?, ?, ?)
            ''', (record.session_id, record.timestamp, record.role, record.content, record.tags))
            row_id = cursor.lastrowid

            text_to_embed = f"{record.tags} {record.content}"
            embedding = model.encode(text_to_embed).tolist()
            embedding_bytes = struct.pack(f"{len(embedding)}f", *embedding)

            cursor.execute('''
                INSERT INTO memory_vec (rowid, embedding)
                VALUES (?, ?)
            ''', (row_id, embedding_bytes))
            inserted_count += 1

        conn.commit()
        conn.close()
        return {"status": "success", "inserted_count": inserted_count}
    except Exception as e:
        print(f"Import 오류 발생: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))
