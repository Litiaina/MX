export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

export interface FieldDefinition {
  uid: string;
  key: string;
  label: string;
  field_type: 'text' | 'long_text' | 'integer' | 'decimal' | 'date' | 'boolean' | 'select' | 'auto_number' | 'attachments';
  required: boolean;
  unique_value: boolean;
  searchable: boolean;
  sortable: boolean;
  table_visible: boolean;
  table_priority: number;
  position: number;
  active: boolean;
  config: Record<string, JsonValue>;
}

export interface SystemFieldDefinition {
  key: string;
  label: string;
  field_type: string;
  searchable: boolean;
  sortable: boolean;
  table_visible: boolean;
  table_priority: number;
  position: number;
}

export interface SchemaResponse {
  revision: number;
  fields: FieldDefinition[];
  system_fields: SystemFieldDefinition[];
}

export interface FileAttachment {
  uid: string;
  file_name: string;
  mime_type: string;
  size: number;
  object_key: string;
  version_id: string | null;
  attachment_field_uid: string | null;
  attachment_field_label: string | null;
  attachment_field_storage_name: string | null;
}

export interface MxRecord {
  uid: string;
  values: Record<string, JsonValue>;
  field_revisions: Record<string, number>;
  attached_files: FileAttachment[];
}

export interface FieldConflict {
  field_uid: string;
  field_key: string;
  label: string;
  base_revision: number;
  current_revision: number;
  current_value: JsonValue;
  your_value: JsonValue;
}

export interface RecordPage {
  data: MxRecord[];
  page: number;
  limit: number;
  has_next: boolean;
  total: number;
  total_pages: number;
}

export interface DeploymentConfig {
  branding: { display_name: string; subtitle: string; organization_name: string; logo_url: string };
  appearance: { preset: string; primary_color: string; sidebar_color: string; radius: string; density: string; default_theme: string; content_width: string };
  terminology: { record_singular: string; record_plural: string; dashboard_label: string; administration_label: string };
  navigation: { show_dashboard: boolean; show_records: boolean; show_quick_actions: boolean; default_workspace: string };
}

export interface DeploymentResponse { revision: number; config: DeploymentConfig; updated_at: number }

export interface DashboardWidget {
  uid: string;
  kind: 'action_rate' | 'attachment_presence';
  display_mode: 'metric' | 'bar' | 'line' | 'pie' | 'donut' | 'progress' | 'table';
  title: string;
  group_field_uid: string | null;
  action_field_uid: string | null;
  action_mode: 'all' | 'nonempty' | 'empty' | 'equals' | 'not_equals' | null;
  action_value: string | null;
  attachment_field_uid: string | null;
  date_field_uid: string | null;
  time_bucket: 'day' | 'week' | 'month' | 'quarter' | 'year' | null;
  chart_value: 'matched' | 'total' | 'rate' | 'breakdown' | null;
  category_limit: number | null;
  show_legend: boolean | null;
  definition?: string | null;
}

export interface DashboardConfigResponse {
  revision: number;
  config: { widgets: DashboardWidget[]; show_user_performance: boolean };
  updated_at: number;
}

export interface ActionRateRow { group: string; total: number; actioned: number; pending: number; rate: number }
export interface UserPerformanceRow {
  actor_uid: string; actor_name: string; actor_email: string;
  records_created: number; records_updated: number; attachments_uploaded: number;
  records_deleted: number; attachment_downloads: number; successful_actions: number;
  failed_actions: number; unique_records_touched: number; last_activity: number;
}

export interface PresenceAccount { uid: string; name: string; access_level: number; access_name: string }
export interface PresenceResponse { accounts: PresenceAccount[]; online: number; at: number }

export interface UserSummary { uid: string; email: string; name: string; access_level: number; access_name: string; totp_enabled: boolean }

export interface StorageLayout {
  revision: number; root: string;
  folder_fields: Pick<FieldDefinition, 'uid' | 'key' | 'label' | 'field_type'>[];
  file_prefix_field: Pick<FieldDefinition, 'uid' | 'key' | 'label' | 'field_type'> | null;
  frozen_records: number; preview: string;
}

export interface BackupEntry { uid: string; file_name: string; object_key: string; size: number; status: string; integrity_check: string; created_at: number; verified_at: number | null }

export interface AuditEntry {
  id: number; event_uid: string; actor_uid: string; actor_name: string; actor_email: string;
  access_level: number; action: string; method: string; path: string; target_type: string | null;
  target_uid: string | null; status_code: number; success: boolean; user_agent: string | null; created_at: number;
}
export interface AuditPage { data: AuditEntry[]; page: number; limit: number; total: number; total_pages: number; has_next: boolean }
