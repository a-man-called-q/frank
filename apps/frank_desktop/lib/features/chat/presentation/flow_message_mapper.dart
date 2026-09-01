import 'package:flow_ui/flow_ui.dart';

import '../../../core/models/workspace_models.dart';

FlowMessageData toFlowMessage(OfficeMessage message) {
  return FlowMessageData.text(
    id: message.id,
    role: message.role == ChatRole.user
        ? FlowMessageRole.user
        : FlowMessageRole.assistant,
    text: message.text,
  ).copyWith(status: _toFlowStatus(message.status));
}

FlowMessageStatus _toFlowStatus(OfficeMessageStatus status) => switch (status) {
  OfficeMessageStatus.pending => FlowMessageStatus.pending,
  OfficeMessageStatus.streaming => FlowMessageStatus.streaming,
  OfficeMessageStatus.complete => FlowMessageStatus.complete,
  OfficeMessageStatus.error => FlowMessageStatus.error,
  OfficeMessageStatus.stopped => FlowMessageStatus.complete,
};
