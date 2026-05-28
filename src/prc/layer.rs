//! Provide an AND layer abstraction that handles building AND circuits in a multiparty computation 
//! (MPC) systems and executing them.
//! 
//! This module defines a `Layer` struct that represents a layer in the MPC system,
//! This module defines Ticket as a way to reserve spots in the layer for AND operations, 
//! This layer provide LayerSource and LayerDest enums to handle input and output data to the layer.


use crate::cor_rnd::BeaverProvider;
use crate::prc::connection::MpcMessageHandler;
use crate::simd_array::{BitArray, D2BitArray};

/// Represents a layer in the multiparty computation (MPC) system.
pub struct Layer {
    // party_const determines if this party handles const additions in mpc
    party_const: bool,
    layer_num: usize,

    current_size: usize,
    state: LayerState,

    and_data: Option<D2BitArray>,
    beaver_data: Option<D2BitArray>,
    operations: Vec<MpcOperation>,
}

/// Keep the internal state of the layer for performing interactive mpc operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerState {
    Ticketing,
    DataInitialized,
    OperationsExecuted,
    SendingBeaverShares,
    BeaverSharesReceived,
    Finished,
}

/// Tickets are used for reserving a fixed number of (AND) gates spots in the layer and accessing
/// the input and output of these gates.
///
/// Every ticket must guarantee that the st_bit is byte aligned
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket {
    layer_num: usize,
    st_bit: usize,
    end_bit: usize,
}

impl Ticket {
    /// Every ticket must guarantee that the st_bit is byte aligned
    pub fn st_byte(&self) -> usize {
        assert!(self.st_bit % 8 == 0, "Start bit must be byte aligned");
        self.st_bit / 8
    }
    pub fn end_byte(&self) -> usize {
        self.end_bit.div_ceil(8)
    }
}

/// Feeds data into the layer as input to  the mpc computation.
/// Can be plaintext input (BitArray), or the output of the previous layer
pub enum LayerSource {
    Input(BitArray),
    TakeZ(Ticket),
}

/// When the operation is done, write the output to destination
pub enum LayerDest {
    Relay { ticket: Ticket, stream: u8 }, // 0 for X, 1 for Y
    None(),                               // Output(&'a mut BitArray),
}

/// Represents the different operations that can be performed in a layer.
/// Operations apply on the input layer of the MPC gates and can be used to manipulate the data
/// when passing between layers.
pub enum MpcOperation {
    /// Copies the input data to the destination.
    Copy {
        inp: LayerSource,
        out: LayerDest,
    },
    /// Replicates the input n times and writes to the destination -> [inp, inp, ..., inp]
    Replicate {
        n: usize,
        inp: LayerSource,
        out: LayerDest,
    },
    /// Replicates each bit of the input n times and writes to the destination -> [inp[0]]*n+[inp[1]]*n  ..., [inp[m]]*n
    BitExpand {
        n: usize,
        inp: LayerSource,
        out: LayerDest,
    },
}

impl Layer {
    pub fn new(party_const: bool, layer_num: usize) -> Self {
        Layer {
            party_const,
            layer_num,

            current_size: 0,
            state: LayerState::Ticketing,

            and_data: None,
            beaver_data: None,

            operations: Vec::new(),
        }
    }

    /// will round-up the size to the minimum byte size need
    ///
    /// # Panics
    /// Panics if the layer is not in the Ticketing state.
    pub fn request_ticket(&mut self, bit_size: usize) -> Ticket {
        if self.state != LayerState::Ticketing {
            panic!("Cannot request ticket when ticketing is not open");
        }

        let st_bit = self.current_size * 8;
        let end_bit = st_bit + bit_size;

        let byte_size = bit_size.div_ceil(8); // Round up to the nearest byte
        self.current_size += byte_size;

        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                "Requesting ticket for layer {}: st_bit: {}, end_bit: {}, current_size: {}",
                self.layer_num,
                st_bit,
                end_bit,
                self.current_size
            );
        }

        Ticket {
            layer_num: self.layer_num,
            st_bit,
            end_bit,
        }
    }

    /// Adds an operation to the layer.
    /// This function should be called only when the layer is in the Ticketing state.
    /// If the layer is not in the Ticketing state, it will panic.
    ///
    /// # Panics
    /// Panics if the layer is not in the Ticketing state.
    #[inline]
    pub fn add_operation(&mut self, op: MpcOperation) {
        if self.state != LayerState::Ticketing {
            panic!("Cannot add operations when not in Ticketing state");
        }
        self.operations.push(op);
    }

    /// Returns (start_bit, array): a mutable reference to the array at the destination stream and the starting loc.
    #[inline]
    pub fn get_mut_array_from_destination(&mut self, dest: LayerDest) -> (usize, &mut BitArray) {
        let LayerDest::Relay { ticket, stream } = dest else {
            panic!("Invalid destination type");
        };
        if stream != 0 && stream != 1 {
            panic!("Stream must be either X(0) or Y(1)");
        }

        self.check_ticket(&ticket);
        let dest = self
            .and_data
            .as_mut()
            .unwrap()
            .get_mut_array(stream as usize);

        (ticket.st_bit, dest)
    }

    /// reads the output of the layer using the given ticket.
    /// 
    /// # Panics
    /// Panics if the ticket is invalid or the layer is not in Finished state
    #[inline]
    pub fn read_output(&self, ticket: &Ticket) -> BitArray {
        if self.state != LayerState::Finished {
            panic!("Cannot read output when layer is not finished");
        }
        self.check_ticket(ticket);

        let mut out = self
            .and_data
            .as_ref()
            .unwrap()
            .get_array(2)
            .to_slice(ticket.st_byte(), ticket.end_byte());
        out.shrink_to_partial_last_byte(ticket.end_bit - ticket.st_bit);
        out
    }

    /// Extracts the source data from a hardcoded BitArray (Input) or the previous layer (TakeZ).
    #[inline]
    pub fn extract_src_data(inp: LayerSource, prev_layer: Option<&Layer>) -> BitArray {
        match inp {
            LayerSource::Input(data) => data,
            LayerSource::TakeZ(ticket) => {
                // read the output of previous layer
                prev_layer
                    .expect("No previous layer found to take Z stream")
                    .read_output(&ticket)
            }
        }
    }
    fn exec_operation(&mut self, op: MpcOperation, prev_layer: Option<&Layer>) {
        match op {
            MpcOperation::Copy { inp, out } => {
                let data = Self::extract_src_data(inp, prev_layer);
                let (start_bit, dest_array) = self.get_mut_array_from_destination(out);
                dest_array.copy_from(start_bit, &data);
            }
            MpcOperation::Replicate { n, inp, out } => {
                let data = Self::extract_src_data(inp, prev_layer);
                let (start_bit, dest_array) = self.get_mut_array_from_destination(out);
                for i in 0..n {
                    dest_array.copy_from(start_bit + i * data.bit_size(), &data);
                }
            }
            MpcOperation::BitExpand { n, inp, out } => {
                let zeroes = BitArray::new(n);
                let ones = BitArray::ones(n);

                let data = Self::extract_src_data(inp, prev_layer);
                let (start_bit, dest_array) = self.get_mut_array_from_destination(out);
                for i in 0..data.bit_size() {
                    if data.get(i) {
                        dest_array.copy_from(start_bit + i * n, &ones);
                    } else {
                        dest_array.copy_from(start_bit + i * n, &zeroes);
                    }
                }
            }
        }
    }

    pub fn check_ticket(&self, ticket: &Ticket) {
        if (self.state != LayerState::DataInitialized) && (self.state != LayerState::Finished) {
            panic!("Cannot use tickets in state {:?}", self.state);
        }
        if ticket.layer_num != self.layer_num {
            panic!("Ticket layer mismatch");
        }
        if ticket.end_byte() > self.current_size {
            panic!("Ticket end exceeds current size");
        }
    }

    /// This function ends the ticketing phase and initializes the and_data and beaver_data.
    /// This function must be called before any operations can be executed.
    ///
    /// # Panics
    /// Panics if the layer is not in the Ticketing state.
    fn end_ticketing_phase(&mut self, beaver_provider: &mut BeaverProvider) {
        // Initialize the and_data and beaver_data with the given size
        assert!(
            self.state == LayerState::Ticketing,
            "Only a layer in the ticketing phase can end ticketing and initialized data"
        );

        self.and_data = Some(D2BitArray::zeros(3, self.current_size * 8));
        self.beaver_data = Some(beaver_provider.take(self.current_size));
        self.state = LayerState::DataInitialized;
    }

    /// Executes all operations in the layer, and ends DataInitialized phased.
    /// This function will execute all operations in the order they were added.
    ///
    /// # Panics
    ///  Panics if the layer is not in the DataInitialized state.
    fn execute_operations(&mut self, prev_layer: Option<&Layer>) {
        assert!(
            self.state == LayerState::DataInitialized,
            "Cannot execute operations before data is initialized"
        );

        while let Some(op) = self.operations.pop() {
            self.exec_operation(op, prev_layer);
        }

        // Move to the next state
        self.state = LayerState::OperationsExecuted;
    }

    /// Performs GMW-based AND operation
    ///
    /// # Panics
    /// Panics if the layer is not in the OperationsExecuted state.
    fn blind_and_with_beaver(&mut self) {
        assert!(
            self.state == LayerState::OperationsExecuted,
            "Cannot start AND exchange before operations are executed"
        );

        let (x, y, _) = self.and_data.as_mut().unwrap().unpack_as_mut_beaver();
        let (a, b, _) = self.beaver_data.as_mut().unwrap().unpack_as_mut_beaver();
        a.inplace_xor(x);
        b.inplace_xor(y);

        self.state = LayerState::SendingBeaverShares;
    }

    /// Returns the beaver shares (u, v) for the interactive reconstruction of the AND operation.
    fn get_beaver_out_shares(&mut self) -> D2BitArray {
        let (u, v, _) = self.beaver_data.as_mut().unwrap().unpack_as_mut_beaver();
        D2BitArray::new(vec![u.clone(), v.clone()])
    }

    /// Takes the beaver shares (u, v) for the interactive reconstruction of the AND operation.
    ///
    /// # Panics
    /// Panics if the layer is not in the SendingBeaverShares state.
    fn recons_beaver_uv(&mut self, beaver_shares: Vec<D2BitArray>) {
        assert!(
            self.state == LayerState::SendingBeaverShares,
            "Cannot receive beaver shares before sending them"
        );

        for beaver_share in beaver_shares {
            self.beaver_data
                .as_mut()
                .unwrap()
                .beaver_xor_d2(&beaver_share);
        }

        self.state = LayerState::BeaverSharesReceived;
    }

    /// Finalizes the layer
    /// This function computes the final result of the AND operations.
    ///
    /// # Panics
    /// Panics if the layer is not in the BeaverSharesReceived state.
    fn finish_and_exchange(&mut self) {
        assert!(
            self.state == LayerState::BeaverSharesReceived,
            "Cannot finalize and exchange before receiving beaver shares"
        );

        // Compute c value such that : a . b = c
        let (x, y, z) = self.and_data.as_mut().unwrap().unpack_as_mut_beaver();
        let (u, v, c) = self.beaver_data.as_mut().unwrap().unpack_as_mut_beaver();

        if self.party_const {
            // this party is in charge of constant additions and should apply UV
            z.mut_and(u, v); // z = uv
        }
        u.inplace_and(y); // u' = u[y]
        v.inplace_and(x); // v' = v[x]
        z.inplace_xor(u); // z = z ^ [u']
        z.inplace_xor(v); // z = z ^ [v']
        z.inplace_xor(c); // z = z ^ [c]

        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                "Finalizing layer {} with party_const {}: ",
                self.layer_num,
                self.party_const
            );
            log::debug!(" ---- x: {:?} |", x);
            log::debug!(" ---- y: {:?} |", y);
            log::debug!(" ---- z: {:?} |", z);
        }

        self.state = LayerState::Finished;
    }

    /// Perform all the operations in the layer and interact with other parties to compute the AND operation.
    /// This function orchestrate all internal phases of the layer from ticketing to finalization.
    /// 
    /// # Panics
    /// Panics if the layer is not in the Ticketing state.
    pub async fn interact(
        &mut self,
        prev_layer: Option<&Layer>,
        beaver_provider: &mut BeaverProvider,
        message_handler: &mut MpcMessageHandler,
    ) {
        // end ticketing phase and initialize data
        log::debug!(
            "Running end_tickets_and_initiate_data on party {}",
            self.party_const
        );
        self.end_ticketing_phase(beaver_provider);

        // Execute operations in the layer
        log::debug!("Running execute_operations   on party {}", self.party_const);
        self.execute_operations(prev_layer);

        // Start and exchange GMW-based AND operation
        log::debug!(
            "Running blind_and_with_beaver on party {}",
            self.party_const
        );
        self.blind_and_with_beaver();

        // Broadcast beaver triples
        log::debug!(
            "Running get_beaver_out_shares on party {}",
            self.party_const
        );
        let rec_shares = self.get_beaver_out_shares();
        log::debug!("Running send_beaver_shares on party {}", self.party_const);
        message_handler.send_beaver_shares(rec_shares).await;

        // Wait for beaver shares from other parties
        log::debug!(
            "Running receive_beaver_shares on party {}",
            self.party_const
        );
        let rec_shares = message_handler.receive_beaver_shares().await;
        // Send beaver triples to the other layer
        log::debug!("Running recons_beaver_uv  on party {}", self.party_const);
        self.recons_beaver_uv(rec_shares);

        // Finalize and exchange results
        log::debug!("Running finish_and_exchange  on party {}", self.party_const);
        self.finish_and_exchange();
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn test_mock_gmw_and() {
        let max_byte_size = 1024;
        let mut beaver_provider1 = BeaverProvider::new();
        let mut beaver_provider2 = BeaverProvider::new();
        beaver_provider1.generate_as_dealer(max_byte_size * 8, &[1; 32], vec![&[2; 32]]);
        beaver_provider2.generate_with_seed(max_byte_size * 8, &[2; 32]);

        let mut rng = ChaCha8Rng::from_seed([42; 32]);
        let bit_size = 256 + 32;

        // Generate ground truth arrays
        let x = BitArray::random(bit_size, &mut rng);
        let y = BitArray::random(bit_size, &mut rng);
        let expected = BitArray::and(&x, &y);

        // Secret share a and b for two parties
        let mut x1 = x.clone();
        let mut y1 = y.clone();
        let x2 = x1.inplace_secret_share(2, &mut rng).pop().unwrap();
        let y2 = y1.inplace_secret_share(2, &mut rng).pop().unwrap();

        {
            // check if reconstructed values match
            let mut x_rec = x1.clone();
            let mut y_rec = y1.clone();
            x_rec.inplace_reconstruct(&[x2.clone()]);
            y_rec.inplace_reconstruct(&[y2.clone()]);
            assert_eq!(x, x_rec, "Reconstructed x does not match original");
            assert_eq!(y, y_rec, "Reconstructed y does not match original");
        }

        // Create two layers
        let mut layer1 = Layer::new(true, 1);
        let mut layer2 = Layer::new(false, 1);

        // Request tickets
        let ticket1 = layer1.request_ticket(x1.bit_size());
        let ticket2 = layer2.request_ticket(x2.bit_size());
        layer1.add_operation(MpcOperation::Copy {
            inp: LayerSource::Input(x1),
            out: LayerDest::Relay {
                ticket: ticket1,
                stream: 0, // X stream
            },
        });
        layer1.add_operation(MpcOperation::Copy {
            inp: LayerSource::Input(y1),
            out: LayerDest::Relay {
                ticket: ticket1,
                stream: 1, // X stream
            },
        });
        layer2.add_operation(MpcOperation::Copy {
            inp: LayerSource::Input(x2),
            out: LayerDest::Relay {
                ticket: ticket2,
                stream: 0, // X stream
            },
        });
        layer2.add_operation(MpcOperation::Copy {
            inp: LayerSource::Input(y2),
            out: LayerDest::Relay {
                ticket: ticket2,
                stream: 1, // X stream
            },
        });

        // end ticketing phase and initialize data
        layer1.end_ticketing_phase(&mut beaver_provider1);
        layer2.end_ticketing_phase(&mut beaver_provider2);

        layer1.execute_operations(None);
        layer2.execute_operations(None);

        // Perform GMW-based AND operation
        layer1.blind_and_with_beaver();
        layer2.blind_and_with_beaver();

        // Beaver out
        let l1_out = layer1.get_beaver_out_shares();
        let l2_out = layer2.get_beaver_out_shares();

        // Beaver in
        layer1.recons_beaver_uv(vec![l2_out]);
        layer2.recons_beaver_uv(vec![l1_out]);

        // Finalize
        layer1.finish_and_exchange();
        layer2.finish_and_exchange();

        // Read outputs
        let output1 = layer1.read_output(&ticket1);
        let output2 = layer2.read_output(&ticket2);

        // Reconstruct outputs
        let mut result = output1.clone();
        result.inplace_reconstruct(&[output2]);

        // Assert correctness
        assert_eq!(expected, result, "GMW AND operation failed");
    }
}
