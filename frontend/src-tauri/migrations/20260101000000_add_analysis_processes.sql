-- Create analysis_processes table for deep meeting analysis
CREATE TABLE IF NOT EXISTS analysis_processes (
    meeting_id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'PENDING',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    error TEXT,
    result TEXT,
    start_time TEXT,
    end_time TEXT,
    processing_time REAL DEFAULT 0.0,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
