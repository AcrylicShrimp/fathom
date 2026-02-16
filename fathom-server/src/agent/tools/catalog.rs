#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolCategory {
    Memory,
    Profile,
    Messaging,
    FileSystem,
    SystemDiscovery,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolCatalogEntry {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) category: ToolCategory,
}

const TOOL_CATALOG: [ToolCatalogEntry; 13] = [
    ToolCatalogEntry {
        name: "memory_append",
        description: "Append a durable note to agent or user long-term memory.",
        category: ToolCategory::Memory,
    },
    ToolCatalogEntry {
        name: "refresh_profile",
        description: "Refresh the session-local immutable profile copy for agent/user/all.",
        category: ToolCategory::Profile,
    },
    ToolCatalogEntry {
        name: "send_message",
        description: "Send a message to the user-facing chat stream. This is a system tool and does not create a follow-up task_done trigger.",
        category: ToolCategory::Messaging,
    },
    ToolCatalogEntry {
        name: "fs_list",
        description: "List files in managed:// or fs:// path.",
        category: ToolCategory::FileSystem,
    },
    ToolCatalogEntry {
        name: "fs_read",
        description: "Read text content from a managed:// or fs:// file path.",
        category: ToolCategory::FileSystem,
    },
    ToolCatalogEntry {
        name: "fs_write",
        description: "Write full text content to a managed:// or fs:// file path.",
        category: ToolCategory::FileSystem,
    },
    ToolCatalogEntry {
        name: "fs_replace",
        description: "Replace text in a managed:// or fs:// file path.",
        category: ToolCategory::FileSystem,
    },
    ToolCatalogEntry {
        name: "sys_get_context",
        description: "Get authoritative runtime/session context and policy hints.",
        category: ToolCategory::SystemDiscovery,
    },
    ToolCatalogEntry {
        name: "sys_get_time",
        description: "Get the latest server clock time context (UTC and local timezone).",
        category: ToolCategory::SystemDiscovery,
    },
    ToolCatalogEntry {
        name: "sys_list_profiles",
        description: "List agent and/or user profiles in the runtime.",
        category: ToolCategory::SystemDiscovery,
    },
    ToolCatalogEntry {
        name: "sys_get_session_identity_map",
        description: "Get active session identity references (agent and participants).",
        category: ToolCategory::SystemDiscovery,
    },
    ToolCatalogEntry {
        name: "sys_get_profile",
        description: "Get a single agent/user profile by id.",
        category: ToolCategory::SystemDiscovery,
    },
    ToolCatalogEntry {
        name: "sys_get_task_payload",
        description: "Lookup the full args/result payload for a task using task_id and part.",
        category: ToolCategory::SystemDiscovery,
    },
];

pub(crate) fn all_tools() -> &'static [ToolCatalogEntry] {
    &TOOL_CATALOG
}

pub(crate) fn known_tool_names() -> Vec<String> {
    TOOL_CATALOG
        .iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

pub(crate) fn discovery_tool_names() -> Vec<String> {
    TOOL_CATALOG
        .iter()
        .filter(|tool| tool.category == ToolCategory::SystemDiscovery)
        .map(|tool| tool.name.to_string())
        .collect()
}
