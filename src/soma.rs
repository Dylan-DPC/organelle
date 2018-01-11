
use std::collections::HashMap;

use super::{
    Result, Protocol, Effector, Handle, Cell, CellMessage, CellRole
};

/// defines constraints on how connections can be made
#[derive(Debug, Copy, Clone)]
pub enum Constraint<R> where
    R: CellRole,
{
    /// require one connection with the specified role
    RequireOne(R),

    /// require any number of connections with the specified role
    Variadic(R),
}

enum ConstraintHandle {
    One(Handle),
    Many(Vec<Handle>),
    Empty,
}

type ConstraintMap<R> = HashMap<R, (ConstraintHandle, Constraint<R>)>;

/// provides core convenience functions with little boilerplate
pub struct Soma<M, R> where
    M: CellMessage,
    R: CellRole,
{
    effector:               Option<Effector<M, R>>,

    inputs:                 ConstraintMap<R>,
    outputs:                ConstraintMap<R>,
}

impl<M, R> Soma<M, R> where
    M: CellMessage,
    R: CellRole,
{
    /// new soma with constraints and default user-defined state
    pub fn new(inputs: Vec<Constraint<R>>, outputs: Vec<Constraint<R>>)
        -> Result<Self>
    {
        Ok(
            Self {
                effector: None,

                inputs: Self::create_roles(inputs)?,
                outputs: Self::create_roles(outputs)?,
            }
        )
    }

    fn init(&mut self, effector: Effector<M, R>) -> Result<()> {
        if self.effector.is_none() {
            self.effector = Some(effector);

            Ok(())
        }
        else {
            bail!("init called twice")
        }
    }

    fn add_input(&mut self, input: Handle, role: R) -> Result<()> {
        Self::add_role(&mut self.inputs, input, role)
    }

    fn add_output(&mut self, output: Handle, role: R) -> Result<()> {
        Self::add_role(&mut self.outputs, output, role)
    }

    fn verify(&self) -> Result<()> {
        if self.effector.is_none() {
            bail!("init was never called");
        }

        Self::verify_constraints(&self.inputs)?;
        Self::verify_constraints(&self.outputs)?;

        Ok(())
    }

    /// update the soma's inputs and outputs, then verify constraints
    ///
    /// if soma handles the given message, it consumes it, otherwise it is
    /// returned so that the cell can use it.
    pub fn update(&mut self, msg: Protocol<M, R>)
        -> Result<Option<Protocol<M, R>>>
    {
        match msg {
            Protocol::Init(effector) => {
                self.init(effector)?;
                Ok(None)
            },

            Protocol::AddInput(input, role) => {
                self.add_input(input, role)?;
                Ok(None)
            },
            Protocol::AddOutput(output, role) => {
                self.add_output(output, role)?;
                Ok(None)
            },
            Protocol::Start => {
                self.verify()?;
                Ok(Some(Protocol::Start))
            },

            msg @ _ => Ok(Some(msg))
        }
    }

    /// get the effector assigned to this cell
    pub fn effector(&self) -> Result<&Effector<M, R>> {
        if self.effector.is_some() {
            Ok(self.effector.as_ref().unwrap())
        }
        else {
            bail!(
                concat!(
                    "effector has not been set ",
                    "(hint: state needs to be updated first)"
                )
            )
        }
    }
    /// convenience function for sending messages by role
    pub fn send_req_input(&self, dest: R, msg: M) -> Result<()> {
        let req_input = self.req_input(dest)?;

        self.effector()?.send(req_input, msg);

        Ok(())
    }

    /// convenience function for sending messages by role
    pub fn send_req_output(&self, dest: R, msg: M) -> Result<()> {
        let req_output = self.req_output(dest)?;

        self.effector()?.send(req_output, msg);

        Ok(())
    }

    /// get a RequireOne input
    pub fn req_input(&self, role: R) -> Result<Handle> {
        Self::get_req(&self.inputs, role)
    }

    /// get a Variadic input
    pub fn var_input(&self, role: R) -> Result<&Vec<Handle>> {
        Self::get_var(&self.inputs, role)
    }

    /// get a RequireOne output
    pub fn req_output(&self, role: R) -> Result<Handle> {
        Self::get_req(&self.outputs, role)
    }

    /// get a Variadic output
    pub fn var_output(&self, role: R) -> Result<&Vec<Handle>> {
        Self::get_var(&self.outputs, role)
    }

    fn create_roles(constraints: Vec<Constraint<R>>)
        -> Result<ConstraintMap<R>>
    {
        let mut map = HashMap::new();

        for c in constraints {
            let result = match c {
                Constraint::RequireOne(role) => map.insert(
                    role,
                    (ConstraintHandle::Empty, Constraint::RequireOne(role))
                ),
                Constraint::Variadic(role) => map.insert(
                    role,
                    (
                        ConstraintHandle::Many(vec![ ]),
                        Constraint::Variadic(role)
                    )
                )
            };

            if result.is_some() {
                bail!("role {:?} specified more than once")
            }
        }

        Ok(map)
    }

    fn add_role(map: &mut ConstraintMap<R>, cell: Handle, role: R)
        -> Result<()>
    {
        if let Some(&mut (ref mut handle, ref constraint))
            = map.get_mut(&role)
        {
            match *constraint {
                Constraint::RequireOne(role) => {
                    let new_hdl = match handle {
                        &mut ConstraintHandle::Empty => {
                            ConstraintHandle::One(cell)
                        },

                        _ => bail!(
                            "only one cell can be assigned to role {:?}",
                            role
                        ),
                    };

                    *handle = new_hdl;
                },
                Constraint::Variadic(role) => match handle {
                    &mut ConstraintHandle::Many(ref mut cells) => {
                        cells.push(cell);
                    },

                    _ => unreachable!("role {:?} was configured wrong", role)
                }
            };

            Ok(())
        }
        else {
            bail!("unexpected role {:?}", role)
        }
    }

    fn verify_constraints(map: &ConstraintMap<R>) -> Result<()> {
        for (_, &(ref handle, ref constraint)) in map.iter() {
            match *constraint {
                Constraint::RequireOne(role) => match handle {
                    &ConstraintHandle::One(_) => (),
                    _ => bail!(
                        "role {:?} does not meet constraint {:?}",
                        role,
                        *constraint
                    )
                },
                Constraint::Variadic(_) => (),
            }
        }

        Ok(())
    }

    fn get_req(map: &ConstraintMap<R>, role: R) -> Result<Handle> {
        if let Some(&(ref handle, Constraint::RequireOne(_))) = map.get(&role)
        {
            match handle {
                &ConstraintHandle::One(ref cell) => Ok(*cell),
                _ => bail!("role {:?} does not meet constraint", role)
            }
        }
        else {
            bail!("unexpected role {:?}", role)
        }
    }

    fn get_var(map: &ConstraintMap<R>, role: R) -> Result<&Vec<Handle>> {
        if let Some(&(ref handle, Constraint::Variadic(_))) = map.get(&role) {
            match handle {
                &ConstraintHandle::Many(ref cells) => Ok(cells),
                _ => unreachable!("role {:?} was configured wrong")
            }
        }
        else {
            bail!("unexpected role {:?}", role)
        }
    }
}

/// cell used to wrap a Soma and a cell specialized with Nucleus
pub struct Eukaryote<M: CellMessage, R: CellRole, N> where
    N: Nucleus<Message=M, Role=R> + Sized + 'static
{
    soma:       Soma<M, R>,
    nucleus:    N,
}

impl<M: CellMessage, R: CellRole, N> Eukaryote<M, R, N> where
    N: Nucleus<Message=M, Role=R> + Sized + 'static
{
    /// wrap a nucleus and constrain the soma
    pub fn new(
        nucleus: N, inputs: Vec<Constraint<R>>, outputs: Vec<Constraint<R>>
    )
        -> Result<Self>
    {
        Ok(
            Self {
                soma: Soma::new(inputs, outputs)?,
                nucleus: nucleus
            }
        )
    }
}

impl<M: CellMessage, R: CellRole, N> Cell for Eukaryote<M, R, N> where
    N: Nucleus<Message=M, Role=R>
{
    type Message = M;
    type Role = R;

    fn update(mut self, msg: Protocol<Self::Message, Self::Role>)
        -> Result<Self>
    {
        if let Some(msg) = self.soma.update(msg)? {
            let nucleus = self.nucleus.update(&self.soma, msg)?;

            Ok(Eukaryote { soma: self.soma, nucleus: nucleus })
        }
        else {
            Ok(self)
        }
    }
}

/// a specialized cell meant to ensure the Soma is always handled correctly
pub trait Nucleus: Sized {
    /// a message that was not handled by the Soma
    type Message: CellMessage;
    /// the role a connection between cells takes
    type Role: CellRole;

    /// update the nucleus with the Soma and cell message
    fn update(
        self,
        soma: &Soma<Self::Message, Self::Role>,
        msg: Protocol<Self::Message, Self::Role>
    )
        -> Result<Self>
    ;
}