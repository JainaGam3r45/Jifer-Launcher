import { useState } from "react";

export function useFormField(initialValue = "") {
  const [value, setValue] = useState(initialValue);

  return {
    value,
    setValue,
    bind: {
      value,
      onChange: (event: React.ChangeEvent<HTMLInputElement>) => setValue(event.target.value)
    }
  };
}
