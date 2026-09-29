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

export interface ModulePermission {
  access_level: number;
  can_read: boolean;
  can_create: boolean;
  can_update: boolean;
  can_delete: boolean;
  can_configure: boolean;
}

export interface ModuleDefinition {
  uid: string;
  slug: string;
  name: string;
  singular_name: string;
  description: string;
  icon: string;
  color: string;
  position: number;
  active: boolean;
  config: Record<string, JsonValue>;
  permissions: ModulePermission[];
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

export interface RecordVersionSummary {
  uid: string; version: number; event: string; actor_uid: string; actor_name: string; created_at: number;
}

export interface RecordVersionDetail extends RecordVersionSummary {
  record_uid: string; module_uid: string; values: Record<string, JsonValue>;
  attachments: { uid: string; file_name: string; mime_type: string; size: number; attachment_field_uid: string | null }[];
}

export interface TrashRecord {
  uid: string; module_uid: string; module_name: string; singular_name: string;
  deleted_at: number; deleted_by_name: string; attachment_count: number;
  summary_fields: TrashSummaryField[];
}

export interface TrashSummaryField { label: string; value: string }
export interface TrashFieldValue {
  uid: string; key: string; label: string; field_type: FieldDefinition['field_type'];
  value: JsonValue; display_value: string; active: boolean;
}
export interface TrashAttachment {
  uid: string; file_name: string; mime_type: string; size: number; field_label: string;
}
export interface TrashRecordDetail extends TrashRecord {
  version_count: number; fields: TrashFieldValue[]; attachments: TrashAttachment[];
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
  collaboration: { message_page_size: number };
}

export interface DeploymentResponse { revision: number; config: DeploymentConfig; updated_at: number }

export interface DashboardWidget {
  uid: string;
  kind: 'action_rate' | 'attachment_presence';
  display_mode: 'metric' | 'bar' | 'line' | 'pie' | 'donut' | 'progress' | 'table';
  title: string;
  module_uid?: string | null;
  group_field_uid: string | null;
  secondary_group_field_uid?: string | null;
  action_field_uid: string | null;
  action_mode: 'all' | 'nonempty' | 'empty' | 'equals' | 'not_equals' | null;
  action_value: string | null;
  attachment_field_uid: string | null;
  date_field_uid: string | null;
  time_bucket: 'day' | 'week' | 'month' | 'quarter' | 'year' | null;
  chart_value: 'matched' | 'pending' | 'total' | 'rate' | 'breakdown' | null;
  result_focus?: 'matched' | 'pending';
  category_limit: number | null;
  show_legend: boolean | null;
  definition?: string | null;
  filters?: DashboardFilter[];
}

export interface DashboardFilter {
  field_uid?: string;
  attachment_field_uid?: string;
  operator: 'equals' | 'not_equals' | 'contains' | 'empty' | 'nonempty' | 'has' | 'missing';
  value?: string | null;
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

export interface NotificationPreferences {
  browser_enabled: boolean;
  sound_enabled: boolean;
  sound_source: 'default' | 'custom' | null;
  sound_volume: number;
  messages: boolean | null;
  mentions: boolean | null;
  channel_activity: boolean | null;
  record_created: boolean | null;
  record_updated: boolean | null;
  record_assigned: boolean | null;
  attachment_received: boolean | null;
  workflow_changes: boolean | null;
  task_activity: boolean | null;
  digest: 'immediate' | 'daily' | 'weekly' | 'off' | null;
  quiet_hours_start: string | null;
  quiet_hours_end: string | null;
}

export interface NotificationSoundInfo {
  exists: boolean;
  file_name?: string;
  mime_type?: string;
  size?: number;
  updated_at?: number;
  url?: string;
}

export interface UserPreferences {
  theme: 'light' | 'dark' | 'system' | null;
  accent_color: string | null;
  auto_scale: boolean | null;
  ui_scale_percent: number | null;
  font_scale_percent: number | null;
  density: 'compact' | 'normal' | 'comfortable' | null;
  content_width: 'standard' | 'wide' | 'full' | null;
  reduced_motion: boolean | null;
  auto_refresh_seconds: number | null;
  default_workspace: string | null;
  notifications: NotificationPreferences;
}

export interface PreferencesResponse { revision: number; preferences: UserPreferences; updated_at: number }

export interface MxNotification {
  uid: string; kind: string; title: string; body: string;
  actor_uid: string | null; actor_name: string | null;
  target_type: string | null; target_uid: string | null; module_uid: string | null;
  data: JsonValue; created_at: number; read_at: number | null;
}

export interface CollaborationChannel {
  uid: string; kind: 'channel' | 'group' | 'direct'; name: string; description: string;
  created_by: string; created_at: number; role: 'owner' | 'admin' | 'member';
  notification_level: 'all' | 'mentions' | 'muted'; invite_policy: 'owner' | 'admins' | 'members'; member_count: number;
  unread_count: number; last_message_at: number | null; direct_user_uid: string | null;
}
export interface CollaborationPerson { uid: string; name: string; profile_photo_updated_at?: number | null }

export interface ChannelMember {
  user_uid: string; user_name: string; profile_photo_updated_at?: number | null; role: 'owner' | 'admin' | 'member'; joined_at: number;
}

export interface ChannelReadState {
  user_uid: string; user_name: string; last_read_at: number; last_read_message_id: number;
}

export interface GlobalSearchResult {
  uid: string; module_uid: string; module_name: string; singular_name: string;
  icon: string; color: string; label: string; context: string;
}

export interface MessageFile {
  uid: string; file_name: string; mime_type: string; size: number; created_at: number;
}

export interface ChannelFile extends MessageFile {
  message_uid: string; sender_uid: string; sender_name: string; message_created_at: number;
}

export interface MessageRecordLink {
  uid: string; module_uid: string; module_name: string; singular_name: string; label: string;
}

export interface MessageReplyPreview {
  uid: string; sender_uid: string; sender_name: string; body: string; deleted_at: number | null;
}

export interface MessageReactionUser { uid: string; name: string }
export interface MessageReaction { emoji: string; users: MessageReactionUser[] }
export interface MessageMention { uid: string; name: string }

export interface ChatMessage {
  uid: string; channel_uid: string; sender_uid: string; sender_name: string; body: string;
  sender_profile_photo_updated_at?: number | null;
  reply_to_uid: string | null; reply_preview?: MessageReplyPreview | null; created_at: number; edited_at: number | null;
  deleted_at: number | null; sequence: number; files: MessageFile[]; record_links: MessageRecordLink[]; reactions?: MessageReaction[]; mentions?: MessageMention[];
  pinned_at?: number | null; pinned_by_uid?: string | null; pinned_by_name?: string | null;
}

export interface UserSummary { uid: string; email: string; name: string; access_level: number; access_name: string; totp_enabled: boolean; profile_photo_updated_at?: number | null }

export interface StorageLayout {
  module_uid: string; module_name: string; module_slug: string; configured: boolean;
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
