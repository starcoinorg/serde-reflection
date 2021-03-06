// Copyright (c) Facebook, Inc. and its affiliates
// SPDX-License-Identifier: MIT OR Apache-2.0
part of bincode;

class BincodeDeserializer extends BinaryDeserializer {
  BincodeDeserializer(Uint8List input) : super(input) {}

  int deserialize_len() {
    return input.getUint32(offset);
  }

  int deserialize_variant_index() {
    return input.getUint32(offset);
  }

  void check_that_key_slices_are_increasing(Slice key1, Slice key2) {
    // Not required by the format.
  }
}
