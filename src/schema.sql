-- Per-agent token-usage monitor state. One row per agent instance hierarchy.
-- Note: `token_repeat` and the skill *content* are deliberately NOT stored —
-- `token_repeat` is an argument (passed on each dial / request) and the skill is
-- re-read fresh from the laboratory on each injection so edits are picked up.
CREATE TABLE IF NOT EXISTS arcanum_agents (
    -- The agent instance being monitored.
    agent_instance_hierarchy TEXT PRIMARY KEY,
    -- The injection baseline: total_tokens at the last injection. NULL until the
    -- first skill is loaded and the baseline is established.
    last_total_tokens        BIGINT,
    -- The loaded skill's reference (re-read fresh at each injection). Both NULL
    -- until load_skill.
    laboratory_id            TEXT,
    skill_path               TEXT,
    -- The latest live response id for this agent, refreshed by `mcp arcanum
    -- begin` (per dial) and `load_skill`. The daemon uses it to reach the
    -- laboratory when re-reading the skill. NULL until first seen.
    response_id              TEXT
);
