-- Per-agent token-usage monitor state. One row per agent instance hierarchy.
CREATE TABLE IF NOT EXISTS arcanum_agents (
    -- The agent instance being monitored.
    agent_instance_hierarchy TEXT PRIMARY KEY,
    -- Re-inject the loaded skill each time total_tokens grows past this many
    -- tokens since the last injection.
    token_repeat             BIGINT NOT NULL,
    -- The injection baseline: total_tokens at the last injection. NULL until the
    -- first skill is loaded and the baseline is established.
    last_total_tokens        BIGINT,
    -- The currently loaded skill's SKILL.md content. NULL until load_skill.
    skill_content            TEXT
);
