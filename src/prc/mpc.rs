use crate::{
    cor_rnd::{ArithValueT, BeaverProvider, BooleanValue, DaBitProvider}, prc::{commitment::Commitment, b2a_conv::ConvLayer}, simd_array::BitArray
};

use super::{
    connection::{MpcMessageHandler, NetStat},
    layer::{Layer, LayerDest, LayerSource, MpcOperation},
};

pub struct MpcProvider {
    message_handler: MpcMessageHandler,
    beaver_provider: BeaverProvider,
    dabit_provider: DaBitProvider,
    layers: Vec<Layer>,
    party_const: bool, // true if this party handles constant additions in mpc

    output_layer: Option<Layer>, //keep the last layer after run for extracting output
}

impl MpcProvider {
    pub fn new(
        party_const: bool,
        beaver_provider: BeaverProvider,
        dabit_provider: DaBitProvider,
        message_handler: MpcMessageHandler,
    ) -> Self {
        MpcProvider {
            message_handler,
            beaver_provider,
            dabit_provider,
            layers: Vec::new(),
            party_const,
            output_layer: None,
        }
    }

    pub fn clear(&mut self) {
        self.layers.clear();
        self.output_layer = None;
    }

    fn create_new_layer(&mut self) {
        self.layers
            .push(Layer::new(self.party_const, self.layers.len()));
    }

    fn get_layer(&mut self, layer_id: usize) -> &mut Layer {
        while layer_id >= self.layers.len() {
            self.create_new_layer();
        }
        &mut self.layers[layer_id]
    }

    /// Creates an and request fo two arrays of size bit_size
    pub fn and(&mut self, bit_size: usize, a: LayerSource, b: LayerSource) -> LayerSource {
        if bit_size == 0 {
            panic!("Cannot create AND operation with zero bits.");
        }

        let current_layer = self.get_layer(0);
        let and_ticket = current_layer.request_ticket(bit_size);

        // writes input into the current layer via copy operation
        current_layer.add_operation(MpcOperation::Copy {
            inp: a,
            out: LayerDest::Relay {
                ticket: and_ticket,
                stream: 0,
            },
        });
        current_layer.add_operation(MpcOperation::Copy {
            inp: b,
            out: LayerDest::Relay {
                ticket: and_ticket,
                stream: 1,
            },
        });
        LayerSource::TakeZ(and_ticket)
    }

    /// take a binary secret shared bit_num-bit value and create a one-hot encoded vector
    pub fn ohe_vec(&mut self, idx_bshared: BooleanValue) -> LayerSource {
        self.recursive_ohe_vec(0, idx_bshared.bit_len() as usize, idx_bshared.value())
    }
    fn recursive_ohe_vec(
        &mut self,
        layer_num: usize,
        bit_num: usize,
        idx_bshared: u32,
    ) -> LayerSource {
        if bit_num == 0 {
            panic!("Cannot create OHE vector with zero bits.");
        }
        if bit_num == 1 {
            // if bit_num is 1, just return the index
            let mut ohe = BitArray::new(2);

            // TO perform not -> only the const party should flip the bit
            ohe.set_bit(0, (idx_bshared != 0) ^ self.party_const);
            ohe.set_bit(1, idx_bshared != 0);

            return LayerSource::Input(ohe);
        }

        // Do ceiling division
        let split = bit_num.div_ceil(2);
        // split the index into two parts
        let low_idx_bshared: u32 = idx_bshared & ((1 << split) - 1);
        let high_idx_bshared = idx_bshared >> split;

        if log::log_enabled!(log::Level::Trace) {
            log::trace!(
                "Splitting idx {:3} - {:08b} to lower: {:3} - {:08b} and higher {:3} - {:08b} on party {}",
                idx_bshared,
                idx_bshared,
                low_idx_bshared,
                low_idx_bshared,
                high_idx_bshared,
                high_idx_bshared,
                self.party_const
            );
        }

        // recursively call ohe for both parts
        let low_bit_num = split;
        let high_bit_num = bit_num - split;

        let low_source = self.recursive_ohe_vec(layer_num + 1, low_bit_num, low_idx_bshared);
        let high_source = self.recursive_ohe_vec(layer_num + 1, high_bit_num, high_idx_bshared);

        let current_layer = self.get_layer(layer_num);
        // create a ticket for the current layer to perform and
        let and_ticket = current_layer.request_ticket(1 << bit_num);

        // add operation to the current layer
        // lower repetition
        current_layer.add_operation(MpcOperation::Replicate {
            n: 1 << high_bit_num,
            inp: low_source,
            out: LayerDest::Relay {
                ticket: and_ticket,
                stream: 0,
            },
        });
        // Higher bit expansion
        current_layer.add_operation(MpcOperation::BitExpand {
            n: 1 << low_bit_num,
            inp: high_source,
            out: LayerDest::Relay {
                ticket: and_ticket,
                stream: 1,
            },
        });
        LayerSource::TakeZ(and_ticket)
    }

    pub fn get_output(&self, src: LayerSource) -> BitArray {
        Layer::extract_src_data(src, self.output_layer.as_ref())
    }

    pub async fn run(&mut self) {
        let mut prev_layer = None;
        // should go in reverse order
        while let Some(mut layer) = self.layers.pop() {
            log::debug!("Running interaction on party {}", self.party_const);
            layer
                .interact(
                    prev_layer.as_ref(),
                    &mut self.beaver_provider,
                    &mut self.message_handler,
                )
                .await;

            prev_layer = Some(layer);
        }
        self.output_layer = prev_layer;
    }

    #[allow(non_snake_case)]
    pub async fn run_conv_B2A(&mut self, binary: Vec<BooleanValue>) -> Vec<ArithValueT> {
        let mut conv_layer = ConvLayer::new(self.party_const);

        // binary.into_iter().map(|(bit_len, value)| BooleanValue::new(bit_len, value)).collect::<Vec<BooleanValue>>(),
        conv_layer.interact(
            binary,
            &mut self.dabit_provider,
            &mut self.message_handler,
        ).await
        // arith.into_iter().map(|av| av.to_u32()).collect()
    }

    pub async fn get_then_reset_netstat(&mut self) -> NetStat {
        self.message_handler.get_then_reset_netstat().await
    }

    pub fn get_preproc_stat(&mut self) -> (usize, usize) {
        let bt = self.beaver_provider.report_beaver_usage_then_reset();
        let dabit = self.dabit_provider.report_dabit_usage_then_reset();
        (bt, dabit)
    }

    // broadcast
    pub async fn aggregate_commitments(&mut self, commit: Commitment) -> Option<Commitment>{
        self.message_handler.send_commit_to_agg(commit.clone()).await;
        let mut commitments = self.message_handler.receive_agg_commits().await;
        commitments.push(commit);
        
        //  
        if self.party_const{
            Some(Commitment::aggregate(&commitments))
        } else{
            None
        }
    }


}
