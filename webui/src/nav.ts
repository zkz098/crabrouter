import { t } from "./i18n";

export type PageTab =
  | "overview"
  | "conntrack"
  | "services"
  | "system"
  | "ota"
  | "interfaces"
  | "routing"
  | "dns"
  | "qos"
  | "firewall"
  | "clients"
  | "extensions"
  | "diagnostics";

export type NavGroupKey = "status" | "system" | "network" | "security" | "tools";

export interface NavItem {
  key: PageTab;
  label: () => string;
  desc: () => string;
  icon: string;
}

export interface NavGroup {
  key: NavGroupKey;
  label: () => string;
  icon: string;
  items: NavItem[];
}

export const NAV_GROUPS: NavGroup[] = [
  {
    key: "status",
    label: () => t("navGroupStatus"),
    icon: "i-ri-dashboard-2-line",
    items: [
      {
        key: "overview",
        label: () => t("navOverview"),
        desc: () => t("descOverview"),
        icon: "i-ri-dashboard-3-line",
      },
      {
        key: "conntrack",
        label: () => t("navConntrack"),
        desc: () => t("descConntrack"),
        icon: "i-ri-pulse-line",
      },
      {
        key: "services",
        label: () => t("navServices"),
        desc: () => t("descServices"),
        icon: "i-ri-server-line",
      },
    ],
  },
  {
    key: "system",
    label: () => t("navGroupSystem"),
    icon: "i-ri-settings-3-line",
    items: [
      {
        key: "system",
        label: () => t("navSystemSettings"),
        desc: () => t("descSystemSettings"),
        icon: "i-ri-shield-user-line",
      },
      {
        key: "ota",
        label: () => t("navOta"),
        desc: () => t("descOta"),
        icon: "i-ri-download-cloud-2-line",
      },
    ],
  },
  {
    key: "network",
    label: () => t("navGroupNetwork"),
    icon: "i-ri-global-line",
    items: [
      {
        key: "interfaces",
        label: () => t("navInterfaces"),
        desc: () => t("descInterfaces"),
        icon: "i-ri-ethernet-line",
      },
      {
        key: "routing",
        label: () => t("navRouting"),
        desc: () => t("descRouting"),
        icon: "i-ri-route-line",
      },
      {
        key: "dns",
        label: () => t("navDns"),
        desc: () => t("descDns"),
        icon: "i-ri-compass-3-line",
      },
      {
        key: "qos",
        label: () => t("navQos"),
        desc: () => t("descQos"),
        icon: "i-ri-speed-up-line",
      },
    ],
  },
  {
    key: "security",
    label: () => t("navGroupSecurity"),
    icon: "i-ri-shield-check-line",
    items: [
      {
        key: "firewall",
        label: () => t("navFirewall"),
        desc: () => t("descFirewall"),
        icon: "i-ri-shield-flash-line",
      },
      {
        key: "clients",
        label: () => t("navClients"),
        desc: () => t("descClients"),
        icon: "i-ri-vpn-line",
      },
    ],
  },
  {
    key: "tools",
    label: () => t("navGroupTools"),
    icon: "i-ri-tools-line",
    items: [
      {
        key: "extensions",
        label: () => t("navExtensions"),
        desc: () => t("descExtensions"),
        icon: "i-ri-apps-2-line",
      },
      {
        key: "diagnostics",
        label: () => t("navDiagnostics"),
        desc: () => t("descDiagnostics"),
        icon: "i-ri-terminal-box-line",
      },
    ],
  },
];

export function findNavMeta(tab: PageTab): { group: NavGroup; item: NavItem } {
  for (const group of NAV_GROUPS) {
    const item = group.items.find((i) => i.key === tab);
    if (item) {
      return { group, item };
    }
  }
  return { group: NAV_GROUPS[0], item: NAV_GROUPS[0].items[0] };
}
