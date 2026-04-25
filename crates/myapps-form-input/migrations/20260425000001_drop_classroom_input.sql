-- Drop ClassroomInput tables left behind on environments migrated before
-- FEAT-83 repurposed the app as FormInput. Child table is dropped first so
-- foreign-key checks succeed.

DROP TABLE IF EXISTS classroom_input_inputs;
DROP TABLE IF EXISTS classroom_input_form_types;
DROP TABLE IF EXISTS classroom_input_classrooms;
