import '../../core/models/organization_models.dart';

/// Bounded, pure undo/redo history for organization graph edits.
///
/// The bloc owns async persistence and selection state; this value object only
/// tracks graph snapshots, so it can be tested without Flutter or timers.
class OrganizationHistory {
  OrganizationHistory({this.maxDepth = 50})
    : assert(maxDepth > 0, 'maxDepth must be positive');

  final int maxDepth;
  final List<OrganizationGraph> _undo = [];
  final List<OrganizationGraph> _redo = [];

  bool get canUndo => _undo.isNotEmpty;
  bool get canRedo => _redo.isNotEmpty;

  void clear() {
    _undo.clear();
    _redo.clear();
  }

  void record(OrganizationGraph previous) {
    _undo.add(previous);
    if (_undo.length > maxDepth) _undo.removeAt(0);
    _redo.clear();
  }

  OrganizationGraph? undo(OrganizationGraph current) {
    if (_undo.isEmpty) return null;
    _redo.add(current);
    return _undo.removeLast();
  }

  OrganizationGraph? redo(OrganizationGraph current) {
    if (_redo.isEmpty) return null;
    _undo.add(current);
    return _redo.removeLast();
  }
}
