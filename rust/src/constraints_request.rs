use assignment_request::AssignedVariable;
use flatbuffers::FlatBufferBuilder;
use gadget_call::{
    call_gadget,
    CallbackContext,
    InstanceDescription,
};
use gadget_generated::gadget::{
    self,
    get_size_prefixed_root_as_root,
    Message,
    R1CSRequest,
    R1CSRequestArgs,
    R1CSResponse,
    Root,
    RootArgs,
    VariableValues,
};
use std::slice::Iter;

pub fn make_constants_request(instance: &InstanceDescription) -> R1CSContext {
    let mut builder = &mut FlatBufferBuilder::new_with_capacity(1024);

    let instance = instance.build(&mut builder);

    let request = R1CSRequest::create(&mut builder, &R1CSRequestArgs {
        instance: Some(instance),
    });

    let message = Root::create(&mut builder, &RootArgs {
        message_type: Message::R1CSRequest,
        message: Some(request.as_union_value()),
    });

    builder.finish_size_prefixed(message, None);
    let buf = builder.finished_data();

    let response = call_gadget(&buf).unwrap();

    R1CSContext(response)
}


pub struct R1CSContext(pub CallbackContext);

impl R1CSContext {
    pub fn iter_constraints(&self) -> R1CSIterator {
        R1CSIterator {
            messages_iter: self.0.result_stream.iter(),
            constraints_count: 0,
            next_constraint: 0,
            constraints: None,
        }
    }

    pub fn response(&self) -> Option<R1CSResponse> {
        let buf = self.0.response.as_ref()?;
        let message = get_size_prefixed_root_as_root(buf);
        message.message_as_r1csresponse()
    }
}

type Term<'a> = AssignedVariable<'a>;

pub struct Constraint<'a> {
    pub a: Vec<Term<'a>>,
    pub b: Vec<Term<'a>>,
    pub c: Vec<Term<'a>>,
}

pub struct R1CSIterator<'a> {
    // Iterate over messages.
    messages_iter: Iter<'a, Vec<u8>>,

    // Iterate over constraints in the current message.
    constraints_count: usize,
    next_constraint: usize,
    constraints: Option<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<gadget::Constraint<'a>>>>,
}

impl<'a> Iterator for R1CSIterator<'a> {
    type Item = Constraint<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next_constraint >= self.constraints_count {
            // Grab the next message, or terminate if none.
            let buf: &[u8] = self.messages_iter.next()?;

            // Parse the message, or fail if invalid.
            let constraints = get_size_prefixed_root_as_root(buf)
                .message_as_r1csconstraints().unwrap()
                .constraints().unwrap();

            // Start iterating the elements of the current message.
            self.constraints_count = constraints.len();
            self.next_constraint = 0;
            self.constraints = Some(constraints);
        }

        let constraint = self.constraints.as_ref().unwrap().get(self.next_constraint);
        self.next_constraint += 1;

        fn to_vec<'a>(lc: VariableValues<'a>) -> Vec<Term<'a>> {
            let mut terms = vec![];
            let var_ids: &[u64] = lc.variable_ids().unwrap().safe_slice();
            let elements: &[u8] = lc.elements().unwrap();

            let stride = elements.len() / var_ids.len();
            if stride == 0 { panic!("Empty elements data."); }

            for i in 0..var_ids.len() {
                terms.push(Term {
                    id: var_ids[i],
                    element: &elements[stride * i..stride * (i + 1)],
                });
            }

            terms
        }

        Some(Constraint {
            a: to_vec(constraint.linear_combination_a().unwrap()),
            b: to_vec(constraint.linear_combination_b().unwrap()),
            c: to_vec(constraint.linear_combination_c().unwrap()),
        })
    }
    // TODO: Replace unwrap and panic with Result.
}
