-- Add migration script here
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Create Canvas table
CREATE TABLE Canvas (
    canvas_id TEXT PRIMARY KEY NOT NULL, -- UUID or SHA1 hash
    name TEXT NOT NULL DEFAULT 'Untitled Canvas', -- User-friendly name for the canvas
    owner_user_id INTEGER NOT NULL, -- Reference to the user who created/owns the canvas
    moderated BOOLEAN NOT NULL DEFAULT FALSE, -- True if the canvas is in a moderated state
    event_file_path TEXT NOT NULL DEFAULT '', -- Stores the path to the event file

    FOREIGN KEY (owner_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Create Canvas_Permissions table
CREATE TABLE Canvas_Permissions (
    user_id INTEGER NOT NULL,
    canvas_id TEXT NOT NULL,
    permission_level TEXT NOT NULL, -- 'R', 'W', 'V', 'M', 'O', 'C' (Read, Write, Veto, Moderate, Owner, Co-Owner)

    PRIMARY KEY (user_id, canvas_id), -- A user can only have one permission level per canvas
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (canvas_id) REFERENCES Canvas(canvas_id) ON DELETE CASCADE,

    CHECK (permission_level IN ('R', 'W', 'V', 'M', 'O', 'C'))
);

CREATE INDEX idx_canvas_permissions_canvas_id ON Canvas_Permissions(canvas_id);
