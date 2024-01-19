/* notes



db.add_component
 - register_debug_info
 - self.find_new_family
  - self.family_after_add
   - self.new_family
   - self.update_transfer_graph
  - self.family_after_remove
 - self.resolve_entity_transfer
  > "transfer the entity, create any new family or tables we need"
 - self.set_component_for
  > "set the actual component we are adding, the tables should be created"
  > "and there should be space for the component"







*/

/*

event stream for decision making
 - events are accumulated in a time-ordered stream
 - analysis is performed on the events to determine causal relationships (intent?)
 - rules executed on the analysis produce advice
*/
