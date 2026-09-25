//! Inline SVG icon components.
//!
//! No icon font, no external asset, no extra dependency: every icon is
//! a small `<svg>` element that inherits `currentColor`. The shared
//! `.icon` class in `styles/main.css` sets size and stroke style.

use leptos::prelude::*;

macro_rules! icon {
    ($name:ident, $($path:tt)*) => {
        #[component]
        pub fn $name() -> impl IntoView {
            view! {
                <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
                    $($path)*
                </svg>
            }
        }
    };
}

icon!(IconDashboard,
    <rect x="3" y="3" width="7" height="9" rx="1.5"/>
    <rect x="14" y="3" width="7" height="5" rx="1.5"/>
    <rect x="14" y="12" width="7" height="9" rx="1.5"/>
    <rect x="3" y="16" width="7" height="5" rx="1.5"/>
);
icon!(IconContracts,
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
    <path d="M14 2v6h6"/>
    <path d="M8 13h8M8 17h5"/>
);
icon!(IconReminders,
    <rect x="3" y="5" width="18" height="16" rx="2"/>
    <path d="M16 3v4M8 3v4M3 11h18"/>
);
icon!(IconChannels,
    <path d="M6 8a6 6 0 0 1 12 0c0 7 3 8 3 8H3s3-1 3-8"/>
    <path d="M10 21a2 2 0 0 0 4 0"/>
);
icon!(IconSettings,
    <circle cx="12" cy="12" r="3"/>
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1A1.7 1.7 0 0 0 4.6 9a1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>
);
icon!(IconLogout,
    <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
    <path d="M16 17l5-5-5-5M21 12H9"/>
);
icon!(IconPlus,
    <path d="M12 5v14M5 12h14"/>
);
icon!(IconSearch,
    <circle cx="11" cy="11" r="7"/>
    <path d="M21 21l-4.3-4.3"/>
);
icon!(IconEdit,
    <path d="M12 20h9"/>
    <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>
);
icon!(IconTrash,
    <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
);
icon!(IconCheck,
    <path d="M20 6L9 17l-5-5"/>
);
icon!(IconAlert,
    <path d="M12 9v4M12 17h.01"/>
    <path d="M10.3 3.9L1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>
);
icon!(IconInfo,
    <circle cx="12" cy="12" r="9"/>
    <path d="M12 8h.01M11 12h1v4h1"/>
);
icon!(IconMail,
    <rect x="3" y="5" width="18" height="14" rx="2"/>
    <path d="M3 7l9 6 9-6"/>
);
icon!(IconShield,
    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
    <path d="M9 12l2 2 4-4"/>
);
icon!(IconBolt,
    <path d="M13 2L4 14h7l-1 8 9-12h-7z"/>
);
icon!(IconGlobe,
    <circle cx="12" cy="12" r="9"/>
    <path d="M3 12h18M12 3a15 15 0 0 1 0 18M12 3a15 15 0 0 0 0 18"/>
);
icon!(IconClock,
    <circle cx="12" cy="12" r="9"/>
    <path d="M12 7v5l3 2"/>
);
icon!(IconFile,
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
    <path d="M14 2v6h6"/>
);
icon!(IconChevronRight,
    <path d="M9 6l6 6-6 6"/>
);
icon!(IconClose,
    <path d="M6 6l12 12M18 6L6 18"/>
);
icon!(IconUser,
    <circle cx="12" cy="8" r="4"/>
    <path d="M4 21a8 8 0 0 1 16 0"/>
);
icon!(IconSend,
    <path d="M22 2L11 13"/>
    <path d="M22 2l-7 20-4-9-9-4z"/>
);
icon!(IconDownload,
    <path d="M12 3v12"/>
    <path d="M7 10l5 5 5-5"/>
    <path d="M5 21h14"/>
);

// Brand icons — line-art versions, single colour. Same `.icon` styling
// applies (18px, currentColor). Rendered inline; no font files.
icon!(IconGoogle,
    <path d="M20.5 12.2H12v-2.8h8.3c-.3 4.5-3.9 8.3-8.3 8.3-4.6 0-8.4-3.8-8.4-8.4S7.4 4.9 12 4.9c2.1 0 4 .8 5.5 2.1l2.1-2.1C17.6 3.1 15 2 12 2 6.5 2 2 6.5 2 12s4.5 10 10 10c5.7 0 9.8-4 9.8-9.8 0-.6-.1-1-.1-1z"/>
);
icon!(IconGithub,
    <path d="M9 19c-4 1.5-4-2.5-6-3m12 5v-3.5c0-1 .1-1.4-.5-2 2.8-.3 5.5-1.4 5.5-6a4.6 4.6 0 0 0-1.3-3.2 4.2 4.2 0 0 0-.1-3.2s-1.1-.3-3.5 1.3a12.3 12.3 0 0 0-6.2 0C6.5 2.8 5.4 3.1 5.4 3.1a4.2 4.2 0 0 0-.1 3.2A4.6 4.6 0 0 0 4 9.5c0 4.6 2.7 5.7 5.5 6-.6.6-.6 1.2-.5 2V21"/>
);

icon!(IconEye,
    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
    <circle cx="12" cy="12" r="3"/>
);
icon!(IconEyeOff,
    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/>
    <path d="M1 1l22 22"/>
);
icon!(IconSun,
    <circle cx="12" cy="12" r="4"/>
    <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M6.3 17.7l-1.4 1.4M19.1 4.9l-1.4 1.4"/>
);
icon!(IconMoon,
    <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>
);
